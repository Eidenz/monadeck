//! Text-field focus on the desktop, from the AT-SPI accessibility bus: when an
//! editable widget (entry, text area, terminal…) gains focus, the VR keyboard
//! can pop up. Switching accessibility on makes Qt/GTK/Chromium apps expose
//! their widgets; it costs nothing visible on the desktop.
use std::collections::HashMap;
use std::sync::mpsc;

pub struct FocusEvent {
    pub role: String,
    pub editable: bool,
    pub app: String,
}

pub struct A11y {
    rx: mpsc::Receiver<FocusEvent>,
    pub ok: bool,
}

impl A11y {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let ok = match start_thread(tx) {
            Ok(()) => true,
            Err(e) => {
                log::warn!("a11y: focus tracking off ({e})");
                false
            }
        };
        Self { rx, ok }
    }

    pub fn drain(&self) -> Vec<FocusEvent> {
        self.rx.try_iter().collect()
    }
}

fn start_thread(tx: mpsc::Sender<FocusEvent>) -> Result<(), String> {
    let session = zbus::blocking::Connection::session().map_err(|e| format!("session bus: {e}"))?;
    // Ask the desktop to expose accessibility (screen readers do the same).
    let _ = session.call_method(
        Some("org.a11y.Bus"),
        "/org/a11y/bus",
        Some("org.freedesktop.DBus.Properties"),
        "Set",
        &("org.a11y.Status", "IsEnabled", zbus::zvariant::Value::Bool(true)),
    );
    let addr: String = session
        .call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some("org.a11y.Bus"), "GetAddress", &())
        .map_err(|e| format!("GetAddress: {e}"))?
        .body()
        .deserialize()
        .map_err(|e| format!("GetAddress body: {e}"))?;
    let a11y = zbus::blocking::connection::Builder::address(addr.as_str())
        .map_err(|e| format!("a11y address: {e}"))?
        .build()
        .map_err(|e| format!("a11y bus: {e}"))?;
    a11y.call_method(
        Some("org.a11y.atspi.Registry"),
        "/org/a11y/atspi/registry",
        Some("org.a11y.atspi.Registry"),
        "RegisterEvent",
        &("object:state-changed:focused",),
    )
    .map_err(|e| format!("RegisterEvent: {e}"))?;
    a11y.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "AddMatch",
        &("type='signal',interface='org.a11y.atspi.Event.Object',member='StateChanged'",),
    )
    .map_err(|e| format!("AddMatch: {e}"))?;
    log::info!("a11y: tracking text-field focus on {addr}");
    std::thread::Builder::new()
        .name("a11y-focus".into())
        .spawn(move || {
            let iter = zbus::blocking::MessageIterator::from(&a11y);
            let mut bad_shape_logged = false;
            for msg in iter {
                let Ok(msg) = msg else { continue };
                let hdr = msg.header();
                if hdr.member().map(|m| m.as_str()) != Some("StateChanged") {
                    continue;
                }
                type Body<'a> = (String, i32, i32, zbus::zvariant::Value<'a>, HashMap<String, zbus::zvariant::Value<'a>>);
                let body = msg.body();
                let (detail, d1, _d2, _any, _props) = match body.deserialize::<Body>() {
                    Ok(b) => b,
                    Err(e) => {
                        if !bad_shape_logged {
                            log::debug!("a11y: unexpected event shape: {e}");
                            bad_shape_logged = true;
                        }
                        continue;
                    }
                };
                if detail != "focused" || d1 != 1 {
                    continue;
                }
                let (Some(sender), Some(path)) = (hdr.sender(), hdr.path()) else { continue };
                let (sender, path) = (sender.to_string(), path.to_string());
                let role: String = a11y
                    .call_method(Some(sender.as_str()), path.as_str(), Some("org.a11y.atspi.Accessible"), "GetRoleName", &())
                    .ok()
                    .and_then(|r| r.body().deserialize().ok())
                    .unwrap_or_default();
                let state: Vec<u32> = a11y
                    .call_method(Some(sender.as_str()), path.as_str(), Some("org.a11y.atspi.Accessible"), "GetState", &())
                    .ok()
                    .and_then(|r| r.body().deserialize().ok())
                    .unwrap_or_default();
                const STATE_EDITABLE: u32 = 1 << 7;
                let editable_state = state.first().is_some_and(|w| w & STATE_EDITABLE != 0);
                let role_l = role.to_lowercase();
                let texty = matches!(
                    role_l.as_str(),
                    "text" | "entry" | "password text" | "terminal" | "document text" | "editbar" | "spin button" | "paragraph" | "search box"
                );
                let editable = editable_state || texty;
                let app: String = a11y
                    .call_method(Some(sender.as_str()), path.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &("org.a11y.atspi.Accessible", "Name"))
                    .ok()
                    .and_then(|r| {
                        let body = r.body();
                        let v = body.deserialize::<zbus::zvariant::Value>().ok()?;
                        String::try_from(v).ok()
                    })
                    .unwrap_or_default();
                log::debug!("a11y: focus role={role:?} editable={editable} name={app:?}");
                let _ = tx.send(FocusEvent { role, editable, app });
            }
            log::warn!("a11y: focus listener ended");
        })
        .map_err(|e| format!("spawn: {e}"))?;
    Ok(())
}
