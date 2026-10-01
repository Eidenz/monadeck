//! Monitor layout from the user's Wayland session: `wl_output` + `xdg_output`
//! give each screen's connector name and *logical* position/size, which is the
//! coordinate space the compositor maps absolute pointer devices onto. Used to
//! label screens and to turn a hit on a screen into a desktop cursor position.
//!
//! [`OutputWatch`] keeps the connection open and follows the layout as it
//! changes: a monitor unplugged or switched away by a KVM, another one moving
//! over to take its place, the same monitor coming back.
use wayland_client::backend::WaylandError;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputInfo {
    /// Connector name, e.g. `DP-3`.
    pub name: String,
    /// Human description ("Dell Inc. U2720Q" style).
    pub description: String,
    pub logical_pos: (i32, i32),
    pub logical_size: (i32, i32),
    pub pixel_size: (i32, i32),
}

/// One `wl_output` global, keyed by its registry name.
struct Entry {
    global: u32,
    /// Its first description is complete (`done`).
    ready: bool,
    info: OutputInfo,
    wl: WlOutput,
    xdg: ZxdgOutputV1,
}

struct State {
    manager: ZxdgOutputManagerV1,
    outputs: Vec<Entry>,
    /// Something finished changing since the last look.
    changed: bool,
    /// Outputs removed since the last look (by name), even if already back.
    gone: Vec<String>,
}

impl State {
    fn bind(&mut self, registry: &WlRegistry, qh: &QueueHandle<Self>, global: u32, version: u32) {
        let wl: WlOutput = registry.bind(global, version.min(4), qh, global);
        let xdg = self.manager.get_xdg_output(&wl, qh, global);
        self.outputs.push(Entry { global, ready: false, info: OutputInfo::default(), wl, xdg });
    }

    fn entry(&mut self, global: u32) -> Option<&mut OutputInfo> {
        self.outputs.iter_mut().find(|e| e.global == global).map(|e| &mut e.info)
    }

    fn done(&mut self, global: u32) {
        if let Some(e) = self.outputs.iter_mut().find(|e| e.global == global) {
            e.ready = true;
        }
        self.changed = true;
    }
}

fn release(e: Entry) {
    e.xdg.destroy();
    if e.wl.version() >= 3 {
        e.wl.release();
    }
}

/// The live monitor layout.
pub struct OutputWatch {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
}

impl OutputWatch {
    pub fn connect() -> anyhow::Result<Self> {
        let conn = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let manager: ZxdgOutputManagerV1 = globals.bind(&qh, 1..=3, ())?;
        let mut state = State { manager, outputs: Vec::new(), changed: false, gone: Vec::new() };
        for g in globals.contents().clone_list() {
            if g.interface == WlOutput::interface().name {
                state.bind(globals.registry(), &qh, g.name, g.version);
            }
        }
        // Two roundtrips: one for wl_output events, one for the xdg_output pass.
        queue.roundtrip(&mut state)?;
        queue.roundtrip(&mut state)?;
        state.changed = false;
        Ok(Self { conn, queue, state })
    }

    pub fn outputs(&self) -> Vec<OutputInfo> {
        self.state
            .outputs
            .iter()
            .filter(|e| e.ready || e.info.logical_size != (0, 0))
            .map(|e| e.info.clone())
            .collect()
    }

    /// Take in what the compositor said since the last call, without waiting.
    /// `Some(change)` when something did.
    pub fn poll(&mut self) -> anyhow::Result<Option<Change>> {
        self.queue.dispatch_pending(&mut self.state)?;
        self.conn.flush()?;
        if let Some(guard) = self.queue.prepare_read() {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.queue.dispatch_pending(&mut self.state)?;
        if !std::mem::take(&mut self.state.changed) {
            return Ok(None);
        }
        Ok(Some(Change { outputs: self.outputs(), gone: std::mem::take(&mut self.state.gone) }))
    }
}

/// What a poll saw: the layout now, and the outputs that went away meanwhile.
/// A monitor that drops and comes straight back (a KVM switching modes) is
/// in both: the layout alone may not show it ever left.
pub struct Change {
    pub outputs: Vec<OutputInfo>,
    pub gone: Vec<String>,
}

impl Drop for OutputWatch {
    fn drop(&mut self) {
        for e in self.state.outputs.drain(..) {
            release(e);
        }
        let _ = self.conn.flush();
    }
}

/// Enumerate outputs once. Empty on a non-Wayland session (the viewer then
/// falls back to the geometry the portal reports per stream).
pub fn list() -> Vec<OutputInfo> {
    match OutputWatch::connect() {
        Ok(w) => w.outputs(),
        Err(e) => {
            log::warn!("desktop: wayland output enumeration unavailable: {e}");
            Vec::new()
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(state: &mut Self, registry: &WlRegistry, event: wl_registry::Event, _: &GlobalListContents, _: &Connection, qh: &QueueHandle<Self>) {
        match event {
            wl_registry::Event::Global { name, interface, version } if interface == WlOutput::interface().name => {
                // Reported once its first `done` arrives.
                state.bind(registry, qh, name, version);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(i) = state.outputs.iter().position(|e| e.global == name) {
                    let e = state.outputs.remove(i);
                    if !e.info.name.is_empty() {
                        state.gone.push(e.info.name.clone());
                    }
                    release(e);
                    state.changed = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for State {
    fn event(_: &mut Self, _: &ZxdgOutputManagerV1, _: <ZxdgOutputManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlOutput, u32> for State {
    fn event(state: &mut Self, _: &WlOutput, event: wl_output::Event, global: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Done = event {
            state.done(*global);
            return;
        }
        let Some(o) = state.entry(*global) else { return };
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

impl Dispatch<ZxdgOutputV1, u32> for State {
    fn event(state: &mut Self, _: &ZxdgOutputV1, event: zxdg_output_v1::Event, global: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let zxdg_output_v1::Event::Done = event {
            // xdg_output v1/v2 finish a change here (v3 leaves it to wl_output).
            state.done(*global);
            return;
        }
        let Some(o) = state.entry(*global) else { return };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => o.logical_pos = (x, y),
            zxdg_output_v1::Event::LogicalSize { width, height } => o.logical_size = (width, height),
            zxdg_output_v1::Event::Name { name } => o.name = name,
            zxdg_output_v1::Event::Description { description } => o.description = description,
            _ => {}
        }
    }
}
