// The binding editor's state: the games that have bindings, the binding being
// edited (a game's, or Monadeck's own), and the file work. The Steam walk takes
// seconds and a game can live on a slow (FUSE / NTFS) library, so scanning,
// opening, saving and resetting all run on one worker thread; the page only
// ever reads what came back.
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use monadeck_core::bindings::{self as core, own, BindableGame, BindingDoc, Controller, Hand, Manifest, Mode, Opened};

/// Whose binding the editor has open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Into `BindState::games`.
    Game(usize),
    /// Monadeck's own controls (the playspace drag).
    Monadeck,
}

/// Where a request's files are.
enum Files {
    Game { actions: PathBuf, dirs: Vec<PathBuf> },
    Own,
}

enum Req {
    Scan,
    Open { token: u64, files: Files, ty: &'static str },
    Save { token: u64, files: Files, ty: &'static str, doc: BindingDoc },
    Reset { token: u64, files: Files, ty: &'static str },
}

enum Res {
    Games(Vec<BindableGame>),
    Opened(u64, Result<Opened, String>),
    Saved(u64, Result<PathBuf, String>),
    Reset(u64, Result<bool, String>),
}

struct Worker {
    tx: Sender<Req>,
    rx: Receiver<Res>,
}

impl Worker {
    fn spawn() -> Self {
        let (tx, req_rx) = mpsc::channel::<Req>();
        let (res_tx, rx) = mpsc::channel::<Res>();
        thread::spawn(move || {
            while let Ok(req) = req_rx.recv() {
                let res = match req {
                    Req::Scan => {
                        let mut games = core::bindable_games();
                        games.sort_by(|a, b| b.last_played.unwrap_or(0).cmp(&a.last_played.unwrap_or(0)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
                        Res::Games(games)
                    }
                    Req::Open { token, files: Files::Game { actions, dirs }, ty } => Res::Opened(token, core::open(&actions, &dirs, ty)),
                    Req::Open { token, files: Files::Own, ty } => Res::Opened(token, own::open(ty)),
                    Req::Save { token, files: Files::Game { dirs, .. }, ty, doc } => Res::Saved(token, core::save_personal(&dirs, ty, &doc)),
                    Req::Save { token, files: Files::Own, ty, doc } => Res::Saved(token, own::save(ty, &doc)),
                    Req::Reset { token, files: Files::Game { dirs, .. }, ty } => Res::Reset(token, core::reset_personal(&dirs, ty)),
                    Req::Reset { token, files: Files::Own, ty } => Res::Reset(token, own::reset(ty)),
                };
                if res_tx.send(res).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx }
    }
}

/// A picker or question over the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum Popup {
    /// [+] on an input: how to use it.
    AddMode { hand: Hand, input: String },
    /// A card's mode, to read it another way.
    ChangeMode { index: usize },
    /// A card's slot: which action it drives.
    Action { index: usize, slot: &'static str },
    /// Which controller's binding to edit.
    Controller,
    Haptics,
    Poses,
    /// Buttons held together (Monadeck's own bindings).
    Chords,
    /// Leaving with unsaved changes.
    Leave(Leave),
}

/// Where the editor was going when it asked about unsaved changes.
#[derive(Clone, Debug, PartialEq)]
pub enum Leave {
    ToList,
    ToController(&'static str),
}

pub struct Editor {
    pub target: Target,
    pub ctrl: &'static Controller,
    pub manifest: Manifest,
    pub doc: BindingDoc,
    /// As last opened or saved: `doc != saved` means unsaved changes.
    pub saved: BindingDoc,
    /// The open binding is the personal one (else the default).
    pub personal: bool,
    /// The action set being shown.
    pub set: String,
    pub mirror: bool,
    pub popup: Option<Popup>,
    /// The input under the pointer (its list entry or its part of the drawing).
    pub hover: Option<(Hand, String)>,
    /// A part of the drawing held under the pointer, and since when (its list
    /// entry scrolls into view after a moment).
    pub spot_dwell: Option<((Hand, String), f64)>,
    /// Scroll the lists to this input.
    pub scroll_to: Option<(Hand, String)>,
    /// A save / reset is in flight.
    pub busy: bool,
}

impl Editor {
    fn new(target: Target, ctrl: &'static Controller, o: Opened) -> Self {
        // Open on the first set that binds something, else the first shown one.
        let shown: Vec<&core::ActionSet> = o.manifest.sets.iter().filter(|s| !s.hidden || o.doc.bound_count(&s.key) > 0).collect();
        let set = shown
            .iter()
            .find(|s| o.doc.bound_count(&s.key) > 0)
            .or(shown.first())
            .map(|s| s.key.clone())
            .unwrap_or_default();
        Self {
            target,
            ctrl,
            saved: o.doc.clone(),
            doc: o.doc,
            manifest: o.manifest,
            personal: o.personal,
            set,
            mirror: false,
            popup: None,
            hover: None,
            spot_dwell: None,
            scroll_to: None,
            busy: false,
        }
    }

    pub fn own(&self) -> bool {
        self.target == Target::Monadeck
    }

    /// Why it can't be saved yet: Monadeck's own need a way to open the
    /// dashboard, or there'd be no way back in.
    pub fn save_blocker(&self) -> Option<String> {
        (self.own() && !own::drives(&self.doc, own::DASHBOARD)).then(|| format!("Give “{}” a button first: without one there's no way back into the dashboard", self.manifest.action_name(own::DASHBOARD)))
    }

    pub fn dirty(&self) -> bool {
        self.doc != self.saved
    }

    /// The controller passed to edits: mirror mode copies them to the other hand.
    pub fn mirror_ctrl(&self) -> Option<&'static Controller> {
        self.mirror.then_some(self.ctrl)
    }

    /// The action sets to show as tabs (hidden ones only if they bind something).
    pub fn sets(&self) -> Vec<&core::ActionSet> {
        self.manifest.sets.iter().filter(|s| !s.hidden || self.doc.bound_count(&s.key) > 0).collect()
    }

    /// Mandatory actions of a set nothing drives yet.
    pub fn unbound_required(&self, set: &str) -> Vec<&core::Action> {
        let mut bound: Vec<String> = self.doc.sources(set).iter().flat_map(|s| s.outputs.iter().map(|(_, a)| a.to_ascii_lowercase())).collect();
        bound.extend(self.doc.chords(set).into_iter().filter(|c| !c.inputs.is_empty()).map(|c| c.action.to_ascii_lowercase()));
        self.manifest
            .actions
            .iter()
            .filter(|a| a.set == set && a.mandatory && matches!(a.kind, core::ActionKind::Boolean | core::ActionKind::Vector1 | core::ActionKind::Vector2))
            .filter(|a| !bound.contains(&a.key))
            .collect()
    }

    pub fn add(&mut self, hand: Hand, input: &str, mode: Mode) -> usize {
        let m = self.mirror_ctrl();
        let set = self.set.clone();
        self.doc.add_source(&set, hand, input, mode, m)
    }

    pub fn bind(&mut self, index: usize, slot: &str, action: Option<&str>) {
        let m = self.mirror_ctrl();
        let set = self.set.clone();
        self.doc.set_output(&set, index, slot, action, m);
    }

    pub fn change_mode(&mut self, index: usize, mode: Mode) {
        let m = self.mirror_ctrl();
        let set = self.set.clone();
        self.doc.change_mode(&set, index, mode, m);
    }

    pub fn remove(&mut self, index: usize) {
        let m = self.mirror_ctrl();
        let set = self.set.clone();
        self.doc.remove_source(&set, index, m);
    }
}

/// What the editor needs about whose binding it is.
pub struct Subject {
    pub name: String,
    /// Controller types it has bindings for.
    pub controllers: Vec<&'static str>,
    /// Of those, the ones with a personal binding.
    pub personal: Vec<&'static str>,
}

pub struct BindState {
    worker: Option<Worker>,
    pub games: Vec<BindableGame>,
    pub scanning: bool,
    pub scanned: bool,
    /// The controller type the user is holding (from the live interaction profile).
    pub holding: Option<&'static str>,
    /// A hand is an UdCap glove (Monadeck's own bindings open on those).
    pub gloves: bool,
    /// Monadeck's own controller types with a personal binding.
    pub own_personal: Vec<&'static str>,
    /// Monadeck's own bindings changed on disk: the overlay reloads them.
    pub own_changed: bool,
    pub editor: Option<Editor>,
    /// A binding being opened: (whose, controller type).
    pub opening: Option<(Target, &'static str)>,
    pub error: Option<String>,
    /// The list shows only games with a personal binding.
    pub only_personal: bool,
    /// A flash message for the page to raise (it owns `LibState::flash`).
    pub notice: Option<String>,
    /// Open this game's editor once the scan lands (from a game's page).
    pub open_when_scanned: Option<(Option<String>, Option<String>)>,
    /// Where to go once the save in flight lands ("save and leave").
    pub after_save: Option<Leave>,
    /// The action set to show once the binding being opened lands.
    open_set: Option<&'static str>,
    token: u64,
    /// The controller drawings, uploaded on first use.
    pub art: std::collections::HashMap<&'static str, egui::TextureHandle>,
}

/// A controller of whose binding it is.
pub fn controller_for(target: Target, ty: &str) -> Option<&'static Controller> {
    match target {
        Target::Game(_) => core::controller(ty),
        Target::Monadeck => own::controller(ty),
    }
}

impl BindState {
    pub fn new() -> Self {
        Self {
            worker: None,
            games: Vec::new(),
            scanning: false,
            scanned: false,
            holding: None,
            gloves: false,
            own_personal: own_personal(),
            own_changed: false,
            editor: None,
            opening: None,
            error: None,
            only_personal: false,
            notice: None,
            open_when_scanned: None,
            after_save: None,
            open_set: None,
            token: 0,
            art: Default::default(),
        }
    }

    /// Re-read which of Monadeck's own bindings are personal (changed on disk).
    pub fn refresh_own(&mut self) {
        self.own_personal = own_personal();
    }

    /// (Re)scan the libraries for games with bindings.
    pub fn scan(&mut self) {
        self.own_personal = own_personal();
        let w = self.worker.get_or_insert_with(Worker::spawn);
        if !self.scanning && w.tx.send(Req::Scan).is_ok() {
            self.scanning = true;
        }
    }

    pub fn subject(&self, target: Target) -> Option<Subject> {
        match target {
            Target::Game(i) => self.games.get(i).map(|g| Subject { name: g.name.clone(), controllers: g.controllers.clone(), personal: g.personal.clone() }),
            Target::Monadeck => Some(Subject { name: "Monadeck".into(), controllers: own::CONTROLLERS.iter().map(|c| c.ty).collect(), personal: self.own_personal.clone() }),
        }
    }

    fn files(&self, target: Target) -> Option<Files> {
        match target {
            Target::Game(i) => self.games.get(i).map(|g| Files::Game { actions: g.actions_path.clone(), dirs: g.dirs.clone() }),
            Target::Monadeck => Some(Files::Own),
        }
    }

    /// Take whatever the worker finished. Call every frame.
    pub fn poll(&mut self) {
        let Some(w) = &self.worker else { return };
        let results: Vec<Res> = w.rx.try_iter().collect();
        for res in results {
            match res {
                Res::Games(games) => {
                    // Keep an open game's editor on it across a rescan.
                    let open = self.editor.as_ref().and_then(|e| match e.target {
                        Target::Game(i) => self.games.get(i).map(|g| g.actions_path.clone()),
                        Target::Monadeck => None,
                    });
                    self.games = games;
                    if let Some(path) = open {
                        match self.games.iter().position(|g| g.actions_path == path) {
                            Some(i) => {
                                if let Some(e) = self.editor.as_mut() {
                                    e.target = Target::Game(i);
                                }
                            }
                            None => self.editor = None,
                        }
                    }
                    self.scanning = false;
                    self.scanned = true;
                    if let Some((app, shortcut)) = self.open_when_scanned.take() {
                        if let Some(i) = self.find(app.as_deref(), shortcut.as_deref()) {
                            self.open(Target::Game(i), None);
                        }
                    }
                }
                Res::Opened(token, r) if token == self.token => {
                    let Some((target, ty)) = self.opening.take() else { continue };
                    match (r, controller_for(target, ty)) {
                        (Ok(o), Some(ctrl)) => {
                            let mirror = self.editor.as_ref().is_some_and(|e| e.mirror);
                            let mut e = Editor::new(target, ctrl, o);
                            e.mirror = mirror;
                            if let Some(set) = self.open_set.take() {
                                e.set = set.to_string();
                            }
                            self.editor = Some(e);
                            self.error = None;
                        }
                        (Err(err), _) => self.error = Some(err),
                        (Ok(_), None) => {}
                    }
                }
                Res::Saved(token, r) if token == self.token => {
                    let Some(e) = self.editor.as_mut() else { continue };
                    e.busy = false;
                    match r {
                        Ok(_) => {
                            e.saved = e.doc.clone();
                            e.personal = true;
                            let (ty, target) = (e.ctrl.ty, e.target);
                            match target {
                                Target::Game(i) => {
                                    if let Some(g) = self.games.get_mut(i) {
                                        if !g.personal.contains(&ty) {
                                            g.personal.push(ty);
                                        }
                                        self.notice = Some(format!("Saved · {} uses it next time it starts", g.name));
                                    }
                                }
                                Target::Monadeck => {
                                    self.own_personal = own_personal();
                                    self.own_changed = true;
                                    self.notice = Some("Saved · in use now".into());
                                }
                            }
                            match self.after_save.take() {
                                Some(Leave::ToList) => self.close(),
                                Some(Leave::ToController(ty)) => self.open(target, Some(ty)),
                                None => {}
                            }
                        }
                        Err(err) => {
                            self.after_save = None;
                            self.error = Some(format!("Couldn't save: {err}"));
                        }
                    }
                }
                Res::Reset(token, r) if token == self.token => {
                    let Some(e) = self.editor.as_mut() else { continue };
                    e.busy = false;
                    match r {
                        Ok(_) => {
                            let (target, ty) = (e.target, e.ctrl.ty);
                            match target {
                                Target::Game(i) => {
                                    if let Some(g) = self.games.get_mut(i) {
                                        g.personal.retain(|t| *t != ty);
                                        self.notice = Some(format!("{} is back to its own controls", g.name));
                                    }
                                }
                                Target::Monadeck => {
                                    self.own_personal = own_personal();
                                    self.own_changed = true;
                                    self.notice = Some("Back to Monadeck's default controls".into());
                                }
                            }
                            self.open(target, Some(ty));
                        }
                        Err(err) => self.error = Some(format!("Couldn't reset: {err}")),
                    }
                }
                _ => {} // a stale answer (the user moved on)
            }
        }
    }

    /// The bindable game a library game is, by Steam appid or shortcut id.
    pub fn find(&self, app_id: Option<&str>, shortcut_id: Option<&str>) -> Option<usize> {
        self.games.iter().position(|g| (app_id.is_some() && g.app_id.as_deref() == app_id) || (shortcut_id.is_some() && g.shortcut_id.as_deref() == shortcut_id))
    }

    /// The controller to open on: gloves for Monadeck's own when you wear them,
    /// else the one in hand, else the backend's usual one, else the first.
    pub fn default_controller(&self, target: Target) -> &'static str {
        let controllers = self.subject(target).map(|s| s.controllers).unwrap_or_default();
        let usual = if monadeck_core::devices::current_backend() == monadeck_core::config::Backend::Wivrn { "oculus_touch" } else { "knuckles" };
        let gloves = (target == Target::Monadeck && self.gloves).then_some(own::GLOVES);
        [gloves, self.holding, Some(usual)].into_iter().flatten().find(|t| controllers.contains(t)).or(controllers.first().copied()).unwrap_or("knuckles")
    }

    /// Open a binding (for `ty`, else the default controller).
    pub fn open(&mut self, target: Target, ty: Option<&'static str>) {
        let Some(files) = self.files(target) else { return };
        let ty = ty.unwrap_or_else(|| self.default_controller(target));
        self.token += 1;
        self.opening = Some((target, ty));
        self.error = None;
        let token = self.token;
        let w = self.worker.get_or_insert_with(Worker::spawn);
        let _ = w.tx.send(Req::Open { token, files, ty });
    }

    /// Open a binding on one of its action sets.
    pub fn open_at(&mut self, target: Target, set: &'static str) {
        self.open_set = Some(set);
        self.open(target, None);
    }

    /// Open the editor for a library game, scanning first if need be.
    pub fn open_for(&mut self, app_id: Option<String>, shortcut_id: Option<String>) {
        self.editor = None;
        if self.scanned {
            if let Some(i) = self.find(app_id.as_deref(), shortcut_id.as_deref()) {
                self.open(Target::Game(i), None);
            }
        } else {
            self.open_when_scanned = Some((app_id, shortcut_id));
            self.scan();
        }
    }

    pub fn save(&mut self) {
        let Some(e) = self.editor.as_ref() else { return };
        let (target, ty, doc) = (e.target, e.ctrl.ty, e.doc.clone());
        let Some(files) = self.files(target) else { return };
        self.token += 1;
        let token = self.token;
        if let Some(e) = self.editor.as_mut() {
            e.busy = true;
        }
        let w = self.worker.get_or_insert_with(Worker::spawn);
        let _ = w.tx.send(Req::Save { token, files, ty, doc });
    }

    /// Back to the default binding (yours is kept as a `.bak` file).
    pub fn reset(&mut self) {
        let Some(e) = self.editor.as_ref() else { return };
        let (target, ty) = (e.target, e.ctrl.ty);
        let Some(files) = self.files(target) else { return };
        self.token += 1;
        let token = self.token;
        if let Some(e) = self.editor.as_mut() {
            e.busy = true;
        }
        let w = self.worker.get_or_insert_with(Worker::spawn);
        let _ = w.tx.send(Req::Reset { token, files, ty });
    }

    /// Show a ready-made editor (the preview rig; no worker, no files).
    pub fn show(&mut self, games: Vec<BindableGame>, target: Target, ctrl: &'static Controller, opened: Opened) {
        self.games = games;
        self.scanned = true;
        self.editor = Some(Editor::new(target, ctrl, opened));
    }

    pub fn close(&mut self) {
        self.editor = None;
        self.opening = None;
        self.error = None;
    }
}

fn own_personal() -> Vec<&'static str> {
    own::CONTROLLERS.iter().map(|c| c.ty).filter(|t| own::has_personal(t)).collect()
}
