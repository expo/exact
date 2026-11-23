//! The desktop settings snapshot and its subsequent changes share one bus connection.
use super::{Bus, Value, PORTAL, PORTAL_PATH};
use crate::desktop::{SettingsWatch, WindowSettings};
use std::time::Duration;

const INTERFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const GNOME: &str = "org.gnome.desktop.interface";
const CURSOR: [&str; 3] = ["cursor-blink", "cursor-blink-time", "cursor-blink-timeout"];

fn setting(settings: &mut WindowSettings, ns: &str, key: &str, value: &Value) {
    // Read and SettingChanged can wrap a variant around the setting.
    let mut value = value;
    while let Value::Variant(inner) = value {
        value = inner;
    }
    if ns == APPEARANCE && key == "color-scheme" {
        settings.dark = match value {
            Value::U32(1) => Some(true),
            Value::U32(2) => Some(false),
            _ => None,
        };
    } else if ns == GNOME {
        let number = match value {
            Value::Num(n) => u64::try_from(*n).ok(),
            Value::U32(n) => Some(u64::from(*n)),
            _ => None,
        };
        match key {
            "cursor-blink" => {
                if let Value::Bool(on) = value {
                    settings.cursor.blink = *on;
                }
            }
            "cursor-blink-time" => {
                if let Some(ms) = number {
                    settings.cursor.cycle = Duration::from_millis(ms);
                }
            }
            "cursor-blink-timeout" => {
                if let Some(s) = number {
                    settings.cursor.timeout = Duration::from_secs(s);
                }
            }
            _ => {}
        }
    }
}

fn read(bus: &mut Bus) -> WindowSettings {
    let mut settings = WindowSettings::default();
    let groups = Value::Array("s".into(), vec![Value::str(APPEARANCE), Value::str(GNOME)]);
    if let Ok(reply) = bus.call(PORTAL, PORTAL_PATH, INTERFACE, "ReadAll", &[groups]) {
        if let [values @ Value::Array(_, _)] = reply.as_slice() {
            for (ns, keys) in [(APPEARANCE, &["color-scheme"][..]), (GNOME, &CURSOR[..])] {
                for key in keys {
                    if let Some(value) = values.get(ns).and_then(|group| group.get(key)) {
                        setting(&mut settings, ns, key, value);
                    }
                }
            }
            bus.queued.clear();
            return settings;
        }
    }
    let args = [Value::str(APPEARANCE), Value::str("color-scheme")];
    let reply = match bus.call(PORTAL, PORTAL_PATH, INTERFACE, "ReadOne", &args) {
        Err(error) if error.starts_with("org.freedesktop.DBus.Error.UnknownMethod") => {
            bus.call(PORTAL, PORTAL_PATH, INTERFACE, "Read", &args)
        }
        other => other,
    };
    if let Ok(body) = reply {
        if let [value] = body.as_slice() {
            setting(&mut settings, APPEARANCE, "color-scheme", value);
            // Only this key was read. Keep queued changes for other settings.
            bus.queued.retain(|m| {
                !matches!(m.body.as_slice(),
                [Value::Str(ns), Value::Str(key), _] if ns == APPEARANCE && key == "color-scheme")
            });
        }
    }
    settings
}

pub(crate) fn watch_settings<T: Copy + Default + PartialEq + Send + 'static>(
    address: &str,
    select: fn(WindowSettings) -> T,
    changed: impl Fn(T) + Send + 'static,
) -> Option<SettingsWatch<T>> {
    const STARTUP: Duration = Duration::from_millis(30);
    let mut bus = Bus::connect(address, STARTUP).ok()?;
    let socket = bus.stream.try_clone().ok()?;
    let (send, receive) = std::sync::mpsc::channel();
    let monitor = move || {
        bus.wait_forever();
        let subscribed = bus.subscribe(&format!(
            "type='signal',sender='{PORTAL}',path='{PORTAL_PATH}',interface='{INTERFACE}',member='SettingChanged'"
        )).is_ok();
        let mut settings = read(&mut bus);
        let mut current = select(settings);
        let _ = send.send(current);
        changed(current);
        if !subscribed {
            return;
        }
        while let Ok(message) = bus.signal() {
            if message.path == PORTAL_PATH
                && message.interface == INTERFACE
                && message.member == "SettingChanged"
            {
                if let [Value::Str(ns), Value::Str(key), value] = message.body.as_slice() {
                    setting(&mut settings, ns, key, value);
                    let next = select(settings);
                    if next != current {
                        current = next;
                        changed(next);
                    }
                }
            }
        }
    };
    std::thread::Builder::new()
        .name("exact-desktop-settings".into())
        .spawn(monitor)
        .ok()?;
    let initial = receive.recv_timeout(STARTUP).unwrap_or_default();
    Some(SettingsWatch { initial, socket })
}
