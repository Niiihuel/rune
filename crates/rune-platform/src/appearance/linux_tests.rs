//! A fake portal `Settings` on a private bus; never the session bus.

use std::sync::Arc;
use std::time::Duration;

use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

use super::*;
use crate::appearance::Appearance;
use crate::portal::tests::bus::Bus;

const WATCHDOG: Duration = Duration::from_secs(5);

struct Settings {
    scheme: u32,
    /// Only the older `Read`, as portals before version 2 answer.
    old: bool,
}

#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl Settings {
    async fn read_one(&self, namespace: String, key: String) -> zbus::fdo::Result<OwnedValue> {
        if self.old {
            return Err(zbus::fdo::Error::UnknownMethod("ReadOne".into()));
        }
        assert_eq!((namespace.as_str(), key.as_str()), (NAMESPACE, KEY));
        Ok(OwnedValue::from(self.scheme))
    }

    async fn read(&self, _namespace: String, _key: String) -> OwnedValue {
        Value::Value(Box::new(Value::U32(self.scheme)))
            .try_into()
            .unwrap()
    }

    #[zbus(signal)]
    async fn setting_changed(
        emitter: &SignalEmitter<'_>,
        namespace: &str,
        key: &str,
        value: Value<'_>,
    ) -> zbus::Result<()>;
}

async fn next(appearance: &Appearance) -> ColorScheme {
    tokio::time::timeout(WATCHDOG, async {
        loop {
            if let Some(scheme) = appearance.poll() {
                return scheme;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("no color scheme arrived")
}

async fn portal(bus: &Bus, scheme: u32, old: bool) -> zbus::Connection {
    let service = bus.connect().await;
    service
        .object_server()
        .at(DESKTOP, Settings { scheme, old })
        .await
        .unwrap();
    service.request_name(DESTINATION).await.unwrap();
    service
}

#[tokio::test]
async fn the_scheme_is_read_then_followed_from_the_portal_only() {
    let bus = Bus::start().await;
    let service = portal(&bus, 1, false).await;
    let appearance = Appearance::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert_eq!(next(&appearance).await, ColorScheme::Dark);
    let emitter = SignalEmitter::new(&service, DESKTOP).unwrap();
    // Another key, another namespace and a stranger change nothing.
    Settings::setting_changed(&emitter, NAMESPACE, "accent-color", Value::U32(2))
        .await
        .unwrap();
    Settings::setting_changed(&emitter, "org.gnome.desktop.interface", KEY, Value::U32(2))
        .await
        .unwrap();
    let stranger = bus.connect().await;
    let forged = SignalEmitter::new(&stranger, DESKTOP).unwrap();
    Settings::setting_changed(&forged, NAMESPACE, KEY, Value::U32(2))
        .await
        .unwrap();
    Settings::setting_changed(&emitter, NAMESPACE, KEY, Value::U32(2))
        .await
        .unwrap();
    assert_eq!(next(&appearance).await, ColorScheme::Light);
    Settings::setting_changed(&emitter, NAMESPACE, KEY, Value::U32(0))
        .await
        .unwrap();
    assert_eq!(next(&appearance).await, ColorScheme::NoPreference);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(appearance.poll(), None);
}

#[tokio::test]
async fn an_older_portal_answers_through_read() {
    let bus = Bus::start().await;
    let _service = portal(&bus, 2, true).await;
    let appearance = Appearance::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert_eq!(next(&appearance).await, ColorScheme::Light);
}

#[tokio::test]
async fn a_portal_that_restarts_is_read_and_followed_again() {
    let bus = Bus::start().await;
    let first = portal(&bus, 1, false).await;
    let appearance = Appearance::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert_eq!(next(&appearance).await, ColorScheme::Dark);
    // The portal goes away: the last scheme stays, nothing is reported.
    first.release_name(DESTINATION).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(appearance.poll(), None);
    // Another takes its name: read at once, then followed.
    let second = portal(&bus, 2, false).await;
    assert_eq!(next(&appearance).await, ColorScheme::Light);
    // The old one's word no longer counts.
    let old = SignalEmitter::new(&first, DESKTOP).unwrap();
    Settings::setting_changed(&old, NAMESPACE, KEY, Value::U32(1))
        .await
        .unwrap();
    let emitter = SignalEmitter::new(&second, DESKTOP).unwrap();
    Settings::setting_changed(&emitter, NAMESPACE, KEY, Value::U32(0))
        .await
        .unwrap();
    assert_eq!(next(&appearance).await, ColorScheme::NoPreference);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(appearance.poll(), None);
}

#[tokio::test]
async fn a_portal_that_comes_later_is_followed() {
    let bus = Bus::start().await;
    let appearance = Appearance::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(appearance.poll(), None);
    let _service = portal(&bus, 2, false).await;
    assert_eq!(next(&appearance).await, ColorScheme::Light);
}

// Linux desktop (escritorio-linux).
#[tokio::test]
async fn a_portal_not_running_is_started_on_demand() {
    let directory = tempfile::tempdir().unwrap();
    let started = directory.path().join("started");
    // It never takes the name: the bus was only asked to start it.
    let exec = format!("/bin/sh -c 'echo > {}'", started.display());
    let bus = Bus::activating(DESTINATION, &exec).await;
    let _appearance = Appearance::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    tokio::time::timeout(WATCHDOG, async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the portal was never started");
}

#[tokio::test]
async fn without_a_portal_nothing_is_reported() {
    let bus = Bus::start().await;
    let appearance = Appearance::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(appearance.poll(), None);
}
