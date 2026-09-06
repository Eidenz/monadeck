//! Monitor layout from the user's Wayland session: `wl_output` + `xdg_output`
//! give each screen's connector name and *logical* position/size, which is the
//! coordinate space the compositor maps absolute pointer devices onto. Used to
//! label screens and to turn a hit on a screen into a desktop cursor position.
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};

#[derive(Clone, Debug, Default)]
pub struct OutputInfo {
    /// Connector name, e.g. `DP-3`.
    pub name: String,
    /// Human description ("Dell Inc. U2720Q" style).
    pub description: String,
    pub logical_pos: (i32, i32),
    pub logical_size: (i32, i32),
    pub pixel_size: (i32, i32),
}

#[derive(Default)]
struct State {
    outputs: Vec<OutputInfo>,
}

/// Enumerate outputs. Empty on a non-Wayland session (the viewer then falls
/// back to the geometry the portal reports per stream).
pub fn list() -> Vec<OutputInfo> {
    match list_inner() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("desktop: wayland output enumeration unavailable: {e}");
            Vec::new()
        }
    }
}

fn list_inner() -> anyhow::Result<Vec<OutputInfo>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let manager: ZxdgOutputManagerV1 = globals.bind(&qh, 1..=3, ())?;

    let mut state = State::default();
    let mut wl_outputs = Vec::new();
    for g in globals.contents().clone_list() {
        if g.interface == WlOutput::interface().name {
            let idx = state.outputs.len();
            state.outputs.push(OutputInfo::default());
            let out: WlOutput = globals.registry().bind(g.name, g.version.min(4), &qh, idx);
            let _xdg = manager.get_xdg_output(&out, &qh, idx);
            wl_outputs.push((out, _xdg));
        }
    }
    // Two roundtrips: one for wl_output events, one for the xdg_output pass.
    queue.roundtrip(&mut state)?;
    queue.roundtrip(&mut state)?;
    for (o, x) in wl_outputs {
        x.destroy();
        if o.version() >= 3 {
            o.release();
        }
    }
    Ok(state.outputs)
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZxdgOutputManagerV1, ()> for State {
    fn event(_: &mut Self, _: &ZxdgOutputManagerV1, _: <ZxdgOutputManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlOutput, usize> for State {
    fn event(state: &mut Self, _: &WlOutput, event: wl_output::Event, idx: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let Some(o) = state.outputs.get_mut(*idx) else { return };
        match event {
            wl_output::Event::Name { name } => {
                if o.name.is_empty() {
                    o.name = name;
                }
            }
            wl_output::Event::Description { description } => {
                if o.description.is_empty() {
                    o.description = description;
                }
            }
            wl_output::Event::Mode { flags, width, height, .. } => {
                let current = flags.into_result().map_or(false, |f| f.contains(wl_output::Mode::Current));
                if current || o.pixel_size == (0, 0) {
                    o.pixel_size = (width, height);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, usize> for State {
    fn event(state: &mut Self, _: &ZxdgOutputV1, event: zxdg_output_v1::Event, idx: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let Some(o) = state.outputs.get_mut(*idx) else { return };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => o.logical_pos = (x, y),
            zxdg_output_v1::Event::LogicalSize { width, height } => o.logical_size = (width, height),
            zxdg_output_v1::Event::Name { name } => o.name = name,
            zxdg_output_v1::Event::Description { description } => o.description = description,
            _ => {}
        }
    }
}
