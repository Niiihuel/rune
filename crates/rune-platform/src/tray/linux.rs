//! The StatusNotifierItem and its dbusmenu, on their own thread.
//!
//! The item is served at `/StatusNotifierItem` under the connection's
//! unique name, which is what it registers with the
//! `org.kde.StatusNotifierWatcher`; a watcher that appears later (a panel
//! restarted) is registered with again. Method calls come from the tray
//! host; whoever calls, the worst they can do is what the menu offers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use rune_i18n::t;
use tokio::{sync::watch, time::timeout};
use zbus::{
    message::Type,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection, MatchRule, MessageStream,
};

use super::mark::{pixmaps, Pixmap};
use super::{TrayEvent, TrayState, TrayStatus};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const RPC_TIMEOUT: Duration = Duration::from_secs(3);

/// The menu's entries, by their dbusmenu IDs.
const OPEN: i32 = 1;
const MUTE: i32 = 3;
const DEAFEN: i32 = 4;
const STATUS: i32 = 5;
/// The statuses follow, from 51 in [`TrayStatus::ALL`]'s order.
const STATUS_FIRST: i32 = 51;
const QUIT: i32 = 7;

pub(super) fn spawn(
    address: Option<String>,
    state: watch::Receiver<TrayState>,
    report: impl Fn(TrayEvent) + Send + Sync + 'static,
) {
    let report: Arc<dyn Fn(TrayEvent) + Send + Sync> = Arc::new(report);
    let failed = report.clone();
    let started = std::thread::Builder::new()
        .name("rune-tray".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                report(TrayEvent::Unavailable);
                return;
            };
            runtime.block_on(async {
                if run(address, state, &report).await.is_none() {
                    report(TrayEvent::Unavailable);
                }
            });
        });
    if started.is_err() {
        failed(TrayEvent::Unavailable);
    }
}

type Report = Arc<dyn Fn(TrayEvent) + Send + Sync>;

/// What both interfaces read: the state, and the menu's revision.
struct Shared {
    state: TrayState,
    revision: u32,
}

async fn run(
    address: Option<String>,
    mut state: watch::Receiver<TrayState>,
    report: &Report,
) -> Option<()> {
    let builder = match &address {
        Some(address) => zbus::connection::Builder::address(address.as_str()),
        None => zbus::connection::Builder::session(),
    }
    .ok()?;
    let connection = timeout(
        RPC_TIMEOUT,
        builder.method_timeout(RPC_TIMEOUT).max_queued(32).build(),
    )
    .await
    .ok()?
    .ok()?;
    let shared = Arc::new(Mutex::new(Shared {
        state: state.borrow_and_update().clone(),
        revision: 1,
    }));
    connection
        .object_server()
        .at(
            ITEM_PATH,
            Item {
                shared: shared.clone(),
                report: report.clone(),
            },
        )
        .await
        .ok()?;
    connection
        .object_server()
        .at(
            MENU_PATH,
            Menu {
                shared: shared.clone(),
                report: report.clone(),
            },
        )
        .await
        .ok()?;
    let mut owners = MessageStream::for_match_rule(
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")
            .ok()?
            .interface("org.freedesktop.DBus")
            .ok()?
            .member("NameOwnerChanged")
            .ok()?
            .add_arg(WATCHER)
            .ok()?
            .build(),
        &connection,
        Some(8),
    )
    .await
    .ok()?;
    report(if register(&connection).await {
        TrayEvent::Shown
    } else {
        TrayEvent::Unavailable
    });
    let item = SignalEmitter::new(&connection, ITEM_PATH).ok()?;
    let menu = SignalEmitter::new(&connection, MENU_PATH).ok()?;
    loop {
        tokio::select! {
            changed = state.changed() => {
                // The handle dropped: the icon goes with the connection.
                changed.ok()?;
                let revision = {
                    let mut shared = shared.lock().ok()?;
                    shared.state = state.borrow_and_update().clone();
                    shared.revision = shared.revision.wrapping_add(1);
                    shared.revision
                };
                let _ = Item::new_icon(&item).await;
                let _ = Item::new_tool_tip(&item).await;
                let _ = Menu::layout_updated(&menu, revision, 0).await;
            }
            message = owners.next() => {
                let Some(Ok(message)) = message else { return Some(()) };
                let Ok((_, _, new)) = message.body().deserialize::<(String, String, String)>() else {
                    continue;
                };
                report(if !new.is_empty() && register(&connection).await {
                    TrayEvent::Shown
                } else {
                    TrayEvent::Unavailable
                });
            }
        }
    }
}

/// Tell the watcher about the item; false without one.
async fn register(connection: &Connection) -> bool {
    let Some(name) = connection.unique_name().map(|name| name.to_string()) else {
        return false;
    };
    matches!(
        timeout(
            RPC_TIMEOUT,
            connection.call_method(
                Some(WATCHER),
                WATCHER_PATH,
                Some(WATCHER),
                "RegisterStatusNotifierItem",
                &(name,),
            ),
        )
        .await,
        Ok(Ok(_))
    )
}

struct Item {
    shared: Arc<Mutex<Shared>>,
    report: Report,
}

impl Item {
    fn state(&self) -> TrayState {
        self.shared
            .lock()
            .map(|shared| shared.state.clone())
            .unwrap_or_default()
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "Communications"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "rune"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        "RUNE"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        pixmaps(self.state().mentions > 0)
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<Pixmap> {
        Vec::new()
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<Pixmap> {
        Vec::new()
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        (
            String::new(),
            Vec::new(),
            "RUNE".to_owned(),
            self.state().summary(),
        )
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).unwrap()
    }

    fn activate(&self, _x: i32, _y: i32) {
        (self.report)(TrayEvent::Open);
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        (self.report)(TrayEvent::Open);
    }

    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: String) {}

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

struct Menu {
    shared: Arc<Mutex<Shared>>,
    report: Report,
}

type Properties = HashMap<String, OwnedValue>;

/// One entry of the menu: its ID, its properties and the entries under it.
struct Entry {
    id: i32,
    properties: Properties,
    children: Vec<Entry>,
}

fn text(value: &str) -> OwnedValue {
    OwnedValue::from(zbus::zvariant::Str::from(value.to_owned()))
}

fn entry(id: i32, label: &str, enabled: bool) -> Entry {
    Entry {
        id,
        properties: HashMap::from([
            ("label".to_owned(), text(label)),
            ("enabled".to_owned(), OwnedValue::from(enabled)),
        ]),
        children: Vec::new(),
    }
}

fn check(id: i32, label: &str, kind: &str, on: bool) -> Entry {
    let mut entry = entry(id, label, true);
    entry
        .properties
        .insert("toggle-type".to_owned(), text(kind));
    entry
        .properties
        .insert("toggle-state".to_owned(), OwnedValue::from(i32::from(on)));
    entry
}

fn separator(id: i32) -> Entry {
    Entry {
        id,
        properties: HashMap::from([("type".to_owned(), text("separator"))]),
        children: Vec::new(),
    }
}

/// The whole menu for `state`.
fn menu(state: &TrayState) -> Entry {
    let _language = rune_i18n::scoped(state.language);
    let signed_in = state.status.is_some();
    let mut status = entry(STATUS, t("Estado"), signed_in);
    if signed_in {
        status
            .properties
            .insert("children-display".to_owned(), text("submenu"));
        status.children = TrayStatus::ALL
            .into_iter()
            .zip(STATUS_FIRST..)
            .map(|(choice, id)| check(id, choice.label(), "radio", state.status == Some(choice)))
            .collect();
    }
    Entry {
        id: 0,
        properties: HashMap::from([("children-display".to_owned(), text("submenu"))]),
        children: vec![
            entry(OPEN, t("Abrir RUNE"), true),
            separator(2),
            check(MUTE, t("Silenciar micrófono"), "checkmark", state.muted),
            check(DEAFEN, t("Ensordecer"), "checkmark", state.deafened),
            status,
            separator(6),
            entry(QUIT, t("Salir de RUNE"), true),
        ],
    }
}

/// An entry as dbusmenu lays it out, `(ia{sv}av)`: each child a variant.
type Node = (i32, Properties, Vec<OwnedValue>);

fn node(entry: Entry) -> Node {
    let children = entry
        .children
        .into_iter()
        .map(|child| {
            let (id, properties, children) = node(child);
            let structure = zbus::zvariant::StructureBuilder::new()
                .add_field(id)
                .add_field(properties)
                .add_field(children)
                .build()
                .expect("a menu entry has fields");
            OwnedValue::try_from(Value::from(structure)).expect("a menu entry has no descriptors")
        })
        .collect();
    (entry.id, entry.properties, children)
}

/// Every entry of `entry`'s tree with its properties.
fn flatten(entry: Entry, into: &mut Vec<(i32, Properties)>) {
    into.push((entry.id, entry.properties));
    for child in entry.children {
        flatten(child, into);
    }
}

/// The event an entry's click means.
fn clicked(id: i32) -> Option<TrayEvent> {
    match id {
        OPEN => Some(TrayEvent::Open),
        MUTE => Some(TrayEvent::ToggleMute),
        DEAFEN => Some(TrayEvent::ToggleDeafen),
        QUIT => Some(TrayEvent::Quit),
        // Any session-bus caller picks `id`: checked, it cannot overflow.
        id => id
            .checked_sub(STATUS_FIRST)
            .and_then(|at| usize::try_from(at).ok())
            .and_then(|at| TrayStatus::ALL.get(at))
            .map(|status| TrayEvent::Status(*status)),
    }
}

impl Menu {
    fn snapshot(&self) -> (u32, TrayState) {
        self.shared
            .lock()
            .map(|shared| (shared.revision, shared.state.clone()))
            .unwrap_or_default()
    }

    fn clicked(&self, id: i32, event: &str) {
        if event != "clicked" {
            return;
        }
        let signed_in = self.snapshot().1.status.is_some();
        match clicked(id) {
            Some(TrayEvent::Status(_)) if !signed_in => {}
            Some(event) => (self.report)(event),
            None => {}
        }
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    /// The whole menu, whichever part is asked for: it is small.
    fn get_layout(&self, _parent: i32, _depth: i32, _properties: Vec<String>) -> (u32, Node) {
        let (revision, state) = self.snapshot();
        (revision, node(menu(&state)))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _properties: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        let mut entries = Vec::new();
        flatten(menu(&self.snapshot().1), &mut entries);
        entries.retain(|(id, _)| ids.is_empty() || ids.contains(id));
        entries
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let mut entries = Vec::new();
        flatten(menu(&self.snapshot().1), &mut entries);
        entries
            .into_iter()
            .find(|(found, _)| *found == id)
            .and_then(|(_, mut properties)| properties.remove(&name))
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("no such property".into()))
    }

    fn event(&self, id: i32, event: String, _data: OwnedValue, _timestamp: u32) {
        self.clicked(id, &event);
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event, _, _) in events {
            self.clicked(id, &event);
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
