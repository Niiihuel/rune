//! A fake tray host on a private bus; never the session bus.

use std::time::Duration;

use tokio::sync::mpsc as async_mpsc;
use zbus::message::Header;

use super::*;
use crate::portal::tests::bus::Bus;
use crate::tray::Tray;

const WATCHDOG: Duration = Duration::from_secs(5);

struct Watcher {
    registered: async_mpsc::UnboundedSender<String>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: Header<'_>,
    ) {
        assert_eq!(
            service,
            header.sender().unwrap().as_str(),
            "its own unique name"
        );
        let _ = self.registered.send(service);
    }
}

async fn watcher(bus: &Bus) -> (zbus::Connection, async_mpsc::UnboundedReceiver<String>) {
    let service = bus.connect().await;
    let (registered, received) = async_mpsc::unbounded_channel();
    service
        .object_server()
        .at(WATCHER_PATH, Watcher { registered })
        .await
        .unwrap();
    service.request_name(WATCHER).await.unwrap();
    (service, received)
}

async fn next(tray: &Tray) -> TrayEvent {
    tokio::time::timeout(WATCHDOG, async {
        loop {
            if let Some(event) = tray.poll() {
                return event;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("no tray event arrived")
}

async fn call<B, R>(
    host: &zbus::Connection,
    item: &str,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> R
where
    B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    host.call_method(Some(item), path, Some(interface), method, body)
        .await
        .unwrap()
        .body()
        .deserialize()
        .unwrap()
}

async fn property(host: &zbus::Connection, item: &str, name: &str) -> OwnedValue {
    call(
        host,
        item,
        ITEM_PATH,
        "org.freedesktop.DBus.Properties",
        "Get",
        &("org.kde.StatusNotifierItem", name),
    )
    .await
}

#[tokio::test]
async fn the_item_registers_shows_the_state_and_reports_its_menu() {
    let bus = Bus::start().await;
    let (host, mut registered) = watcher(&bus).await;
    let tray = Tray::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    let item = tokio::time::timeout(WATCHDOG, registered.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next(&tray).await, TrayEvent::Shown);
    let tooltip = property(&host, &item, "ToolTip").await;
    let (_, _, title, summary): (String, Vec<Pixmap>, String, String) = tooltip.try_into().unwrap();
    assert_eq!((title.as_str(), summary.as_str()), ("RUNE", "Todo leído"));
    tray.update(TrayState {
        mentions: 2,
        unread: 1,
        muted: true,
        deafened: false,
        status: Some(TrayStatus::Idle),
        ..TrayState::default()
    });
    let summary = tokio::time::timeout(WATCHDOG, async {
        loop {
            let tooltip = property(&host, &item, "ToolTip").await;
            let (_, _, _, summary): (String, Vec<Pixmap>, String, String) =
                tooltip.try_into().unwrap();
            if summary != "Todo leído" {
                return summary;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(summary, "2 menciones · 1 conversación sin leer");
    let pixmaps: Vec<Pixmap> = property(&host, &item, "IconPixmap")
        .await
        .try_into()
        .unwrap();
    assert_eq!(
        pixmaps,
        super::super::mark::pixmaps(true),
        "the dot for mentions"
    );
    // The layout: the mute entry is checked, the status a radio submenu.
    let (_, (root, _, children)): (u32, Node) = call(
        &host,
        &item,
        MENU_PATH,
        "com.canonical.dbusmenu",
        "GetLayout",
        &(0i32, -1i32, Vec::<String>::new()),
    )
    .await;
    assert_eq!(root, 0);
    assert_eq!(children.len(), 7);
    let properties: Vec<(i32, Properties)> = call(
        &host,
        &item,
        MENU_PATH,
        "com.canonical.dbusmenu",
        "GetGroupProperties",
        &(vec![MUTE, STATUS_FIRST + 1], Vec::<String>::new()),
    )
    .await;
    let state = |id: i32| {
        properties
            .iter()
            .find(|(found, _)| *found == id)
            .map(|(_, properties)| i32::try_from(&properties["toggle-state"]).unwrap())
    };
    assert_eq!(state(MUTE), Some(1));
    assert_eq!(state(STATUS_FIRST + 1), Some(1), "Ausente is chosen");
    // Clicks: the window, a status and quitting.
    let _: () = call(
        &host,
        &item,
        ITEM_PATH,
        "org.kde.StatusNotifierItem",
        "Activate",
        &(0i32, 0i32),
    )
    .await;
    assert_eq!(next(&tray).await, TrayEvent::Open);
    let click = |id: i32| (id, "clicked", Value::from(0i32), 0u32);
    let _: () = call(
        &host,
        &item,
        MENU_PATH,
        "com.canonical.dbusmenu",
        "Event",
        &click(STATUS_FIRST + 2),
    )
    .await;
    assert_eq!(
        next(&tray).await,
        TrayEvent::Status(TrayStatus::DoNotDisturb)
    );
    let _: () = call(
        &host,
        &item,
        MENU_PATH,
        "com.canonical.dbusmenu",
        "Event",
        &(DEAFEN, "hovered", Value::from(0i32), 0u32),
    )
    .await;
    let _: () = call(
        &host,
        &item,
        MENU_PATH,
        "com.canonical.dbusmenu",
        "Event",
        &click(QUIT),
    )
    .await;
    assert_eq!(next(&tray).await, TrayEvent::Quit, "a hover is not a click");
    // The panel goes and comes back: the item says so and registers again.
    drop(registered);
    host.release_name(WATCHER).await.unwrap();
    assert_eq!(next(&tray).await, TrayEvent::Unavailable);
    let (_again, mut registered) = watcher(&bus).await;
    tokio::time::timeout(WATCHDOG, registered.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next(&tray).await, TrayEvent::Shown);
}

#[tokio::test]
async fn without_a_tray_host_it_is_unavailable_and_statuses_need_an_account() {
    let bus = Bus::start().await;
    let tray = Tray::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert_eq!(next(&tray).await, TrayEvent::Unavailable);
    assert_eq!(
        clicked(STATUS_FIRST + 3),
        Some(TrayEvent::Status(TrayStatus::Invisible))
    );
    assert_eq!(clicked(STATUS_FIRST + 4), None);
    let signed_out = menu(&TrayState::default());
    assert!(signed_out
        .children
        .iter()
        .all(|entry| entry.children.is_empty()));
}
