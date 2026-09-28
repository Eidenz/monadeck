//! The binding editor's shots (`bind-*`): a VRChat-like sample game on each
//! controller, the pickers, the hover line, and the list. With
//! `MONADECK_PREVIEW_GAME=<name>` also `bind-real`: that game from your
//! library, as the editor would open it.
use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use monadeck_core::bindings::{self as core, own, BindableGame, BindingDoc, Hand, Manifest, Mode, Opened};

use super::{sample_games, sample_state, shoot_with, wanted, Tex, MAIN_PX};
use crate::bindings::{Leave, Popup, Target};
use crate::ui::{LibState, Nav};

const MANIFEST: &str = r#"{
  "action_sets": [
    { "name": "/actions/Global", "usage": "leftright" },
    { "name": "/actions/Menu", "usage": "single" },
    { "name": "/actions/Drone", "usage": "leftright" }
  ],
  "actions": [
    { "name": "/actions/Global/in/Move", "type": "vector2", "requirement": "mandatory" },
    { "name": "/actions/Global/in/Rotate", "type": "vector2" },
    { "name": "/actions/Global/in/Jump", "type": "boolean", "requirement": "mandatory" },
    { "name": "/actions/Global/in/Crouch", "type": "boolean", "requirement": "mandatory" },
    { "name": "/actions/Global/in/Main_Menu", "type": "boolean" },
    { "name": "/actions/Global/in/Quick_Menu", "type": "boolean" },
    { "name": "/actions/Global/in/Mic", "type": "boolean" },
    { "name": "/actions/Global/in/Grab", "type": "boolean" },
    { "name": "/actions/Global/in/Use", "type": "boolean" },
    { "name": "/actions/Global/in/Use_Axis", "type": "vector1" },
    { "name": "/actions/Global/in/Grip_Axis", "type": "vector1" },
    { "name": "/actions/Global/in/Gesture_Toggle", "type": "boolean" },
    { "name": "/actions/Global/in/Stick_Click", "type": "boolean" },
    { "name": "/actions/Global/in/Chatbox", "type": "boolean" },
    { "name": "/actions/Global/in/Camera", "type": "boolean" },
    { "name": "/actions/Global/in/Pose", "type": "pose" },
    { "name": "/actions/Global/in/SkeletonLeftHand", "type": "skeleton", "skeleton": "/skeleton/hand/left" },
    { "name": "/actions/Global/in/SkeletonRightHand", "type": "skeleton", "skeleton": "/skeleton/hand/right" },
    { "name": "/actions/Global/out/Haptic", "type": "vibration" },
    { "name": "/actions/Menu/in/Scroll", "type": "vector2" },
    { "name": "/actions/Menu/in/Select", "type": "boolean" },
    { "name": "/actions/Menu/in/Back", "type": "boolean" },
    { "name": "/actions/Drone/in/Fly", "type": "vector2" },
    { "name": "/actions/Drone/in/Up_Down", "type": "vector2" },
    { "name": "/actions/Drone/in/Exit", "type": "boolean" }
  ],
  "default_bindings": [
    { "controller_type": "knuckles", "binding_url": "bindings_knuckles.json" },
    { "controller_type": "oculus_touch", "binding_url": "bindings_oculus_touch.json" },
    { "controller_type": "vive_controller", "binding_url": "bindings_vive_controller.json" }
  ],
  "localization": [ {
    "language_tag": "en_US",
    "/actions/Global": "Global",
    "/actions/Global/in/Move": "Move",
    "/actions/Global/in/Rotate": "Turn",
    "/actions/Global/in/Main_Menu": "Main Menu",
    "/actions/Global/in/Quick_Menu": "Quick Menu",
    "/actions/Global/in/Mic": "Toggle Microphone",
    "/actions/Global/in/Use": "Use / Interact",
    "/actions/Global/in/Use_Axis": "Use (analog)",
    "/actions/Global/in/Gesture_Toggle": "Gesture Toggle",
    "/actions/Global/in/Stick_Click": "Stick Click",
    "/actions/Global/in/Chatbox": "Open Chatbox",
    "/actions/Global/in/Camera": "Open Camera",
    "/actions/Global/in/Pose": "Hand Pose",
    "/actions/Global/out/Haptic": "Controller Vibration",
    "/actions/Menu/in/Scroll": "Scroll Menu",
    "/actions/Drone/in/Up_Down": "Up / Down"
  } ]
}"#;

fn src(hand: &str, input: &str, mode: &str, outs: &[(&str, &str)]) -> serde_json::Value {
    let inputs: serde_json::Map<String, serde_json::Value> = outs.iter().map(|(k, a)| (k.to_string(), serde_json::json!({ "output": a }))).collect();
    serde_json::json!({ "path": format!("/user/hand/{hand}/input/{input}"), "mode": mode, "inputs": inputs })
}

/// The Index binding: the Global set well filled (with a pinch xrizer won't
/// read), Menu and Drone smaller.
fn knuckles() -> BindingDoc {
    let g = |a: &str| format!("/actions/global/in/{a}");
    let (mv, rot, st, ua, us, gr, jp, qm, mm, mic, gt, cb) =
        (g("move"), g("rotate"), g("stick_click"), g("use_axis"), g("use"), g("grab"), g("jump"), g("quick_menu"), g("main_menu"), g("mic"), g("gesture_toggle"), g("chatbox"));
    let global = serde_json::json!({
        "sources": [
            src("left", "thumbstick", "joystick", &[("position", &mv), ("click", &st)]),
            src("right", "thumbstick", "joystick", &[("position", &rot)]),
            src("left", "trigger", "trigger", &[("pull", &ua), ("click", &us)]),
            src("right", "trigger", "trigger", &[("pull", &ua), ("click", &us)]),
            src("left", "grip", "grab", &[("grab", &gr)]),
            src("right", "grip", "grab", &[("grab", &gr)]),
            src("right", "a", "button", &[("click", &jp)]),
            src("right", "b", "button", &[("click", &qm)]),
            src("left", "a", "button", &[("click", &mic)]),
            src("left", "b", "button", &[("click", &mm)]),
            src("right", "trackpad", "button", &[("click", &gt)]),
            src("left", "pinch", "button", &[("click", &cb)]),
        ],
        "haptics": [
            { "output": "/actions/global/out/haptic", "path": "/user/hand/left/output/haptic" },
            { "output": "/actions/global/out/haptic", "path": "/user/hand/right/output/haptic" }
        ],
        "poses": [
            { "output": "/actions/global/in/pose", "path": "/user/hand/left/pose/raw" },
            { "output": "/actions/global/in/pose", "path": "/user/hand/right/pose/raw" }
        ],
        "skeleton": [
            { "output": "/actions/global/in/skeletonlefthand", "path": "/user/hand/left/input/skeleton/left" },
            { "output": "/actions/global/in/skeletonrighthand", "path": "/user/hand/right/input/skeleton/right" }
        ]
    });
    let menu = serde_json::json!({ "sources": [
        src("left", "thumbstick", "joystick", &[("position", "/actions/menu/in/scroll")]),
        src("right", "trigger", "button", &[("click", "/actions/menu/in/select")]),
        src("right", "b", "button", &[("click", "/actions/menu/in/back")]),
    ] });
    let drone = serde_json::json!({ "sources": [
        src("left", "thumbstick", "joystick", &[("position", "/actions/drone/in/fly")]),
        src("right", "thumbstick", "joystick", &[("position", "/actions/drone/in/up_down")]),
    ] });
    let doc = serde_json::json!({
        "controller_type": "knuckles",
        "name": "Default Bindings for Valve Index",
        "description": "",
        "bindings": { "/actions/global": global, "/actions/menu": menu, "/actions/drone": drone }
    });
    BindingDoc::parse(&doc.to_string()).expect("sample binding")
}

fn touch() -> BindingDoc {
    let g = |a: &str| format!("/actions/global/in/{a}");
    let doc = serde_json::json!({
        "controller_type": "oculus_touch",
        "bindings": { "/actions/global": { "sources": [
            src("left", "joystick", "joystick", &[("position", &g("move")), ("click", &g("stick_click"))]),
            src("right", "joystick", "joystick", &[("position", &g("rotate"))]),
            src("left", "trigger", "trigger", &[("pull", &g("use_axis")), ("click", &g("use"))]),
            src("right", "trigger", "trigger", &[("pull", &g("use_axis")), ("click", &g("use"))]),
            src("left", "grip", "button", &[("click", &g("grab"))]),
            src("right", "grip", "button", &[("click", &g("grab"))]),
            src("right", "a", "button", &[("click", &g("jump"))]),
            src("right", "b", "button", &[("click", &g("quick_menu"))]),
            src("left", "x", "button", &[("click", &g("mic"))]),
            src("left", "y", "button", &[("click", &g("main_menu"))]),
            src("left", "application_menu", "button", &[("click", &g("main_menu"))]),
        ] } }
    });
    BindingDoc::parse(&doc.to_string()).expect("sample binding")
}

fn vive() -> BindingDoc {
    let g = |a: &str| format!("/actions/global/in/{a}");
    let doc = serde_json::json!({
        "controller_type": "vive_controller",
        "bindings": { "/actions/global": { "sources": [
            src("left", "trackpad", "trackpad", &[("position", &g("move")), ("click", &g("jump"))]),
            src("right", "trackpad", "dpad", &[("west", &g("rotate")), ("east", &g("rotate"))]),
            src("left", "trigger", "trigger", &[("pull", &g("use_axis"))]),
            src("right", "trigger", "trigger", &[("pull", &g("use_axis"))]),
            src("left", "grip", "button", &[("click", &g("grab"))]),
            src("right", "application_menu", "button", &[("click", &g("quick_menu"))]),
        ] } }
    });
    BindingDoc::parse(&doc.to_string()).expect("sample binding")
}

/// The list: stand-ins for the sample library's games (same ids, so their
/// covers show), two customized, one that isn't in the library.
fn sample_bindables() -> Vec<BindableGame> {
    let all = ["knuckles", "oculus_touch", "vive_controller"];
    let game = |name: &str, app: Option<&str>, personal: bool| BindableGame {
        name: name.into(),
        app_id: app.map(str::to_string),
        shortcut_id: None,
        dirs: vec!["/nowhere".into()],
        actions_path: format!("/nowhere/{name}/actions.json").into(),
        controllers: all.to_vec(),
        personal: if personal { vec!["knuckles"] } else { vec![] },
        last_played: None,
    };
    vec![
        game("VRChat", Some("1000"), true),
        game("Half-Life: Alyx", Some("1001"), false),
        game("Pistol Whip", Some("1004"), false),
        game("Bonelab", Some("1005"), false),
        game("Blade & Sorcery", Some("1008"), false),
        game("Ramage", Some("1011"), true),
        game("GOAT", Some("1012"), false),
        game("Garden of the Sea", None, false),
    ]
}

fn opened(doc: BindingDoc, personal: bool) -> Opened {
    Opened { manifest: Manifest::parse(MANIFEST).expect("sample manifest"), doc, personal, default_path: None, personal_path: None }
}

/// A dashboard on the Bindings page with VRChat's editor open.
fn editing(ctx: &egui::Context, ty: &'static str, doc: BindingDoc, personal: bool) -> LibState {
    let mut st = sample_state();
    st.games = sample_games(ctx);
    st.nav = Nav::Bindings;
    st.binds.holding = Some("knuckles");
    st.binds.show(sample_bindables(), Target::Game(0), core::controller(ty).expect("known controller"), opened(doc, personal));
    st
}

/// The Bindings page on Monadeck's own controls, as shipped.
fn own(ctx: &egui::Context, ty: &'static str) -> LibState {
    let mut st = sample_state();
    st.games = sample_games(ctx);
    st.nav = Nav::Bindings;
    st.binds.gloves = ty == own::GLOVES;
    let o = Opened { manifest: own::manifest(), doc: own::default_doc(ty), personal: false, default_path: None, personal_path: None };
    st.binds.show(sample_bindables(), Target::Monadeck, own::controller(ty).expect("own controller"), o);
    st
}

fn edit(st: &mut LibState, f: impl FnOnce(&mut crate::bindings::Editor)) {
    if let Some(e) = st.binds.editor.as_mut() {
        f(e);
    }
}

/// The source index of `hand`'s `input` in the open set.
fn index_of(st: &LibState, hand: Hand, input: &str) -> usize {
    let e = st.binds.editor.as_ref().expect("editor");
    e.doc.sources(&e.set).iter().find(|s| s.hand == Some(hand) && s.input == input).map_or(0, |s| s.index)
}

pub(super) fn shots(ctx: &egui::Context, textures: &mut HashMap<egui::TextureId, Tex>, dir: &Path) -> Result<()> {
    type Setup = Box<dyn Fn(&egui::Context) -> (LibState, Option<egui::Pos2>)>;
    let mut shots: Vec<(&str, Setup)> = vec![
        ("bind-list", Box::new(|ctx| {
            let mut st = sample_state();
            st.games = sample_games(ctx);
            st.nav = Nav::Bindings;
            st.binds.games = sample_bindables();
            st.binds.scanned = true;
            (st, None)
        })),
        ("bind-editor", Box::new(|ctx| (editing(ctx, "knuckles", knuckles(), true), None))),
        // The pointer on the left trigger's entry: SteamVR's line to the drawing.
        ("bind-hover-list", Box::new(|ctx| (editing(ctx, "knuckles", knuckles(), true), Some(egui::pos2(150.0, 342.0))))),
        // The pointer on the right A button's spot.
        ("bind-hover-spot", Box::new(|ctx| (editing(ctx, "knuckles", knuckles(), true), Some(egui::pos2(0.0, 0.0))))),
        ("bind-menu-set", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| e.set = "/actions/menu".into());
            (st, None)
        })),
        ("bind-default", Box::new(|ctx| (editing(ctx, "knuckles", knuckles(), false), None))),
        ("bind-dirty", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| {
                let i = e.add(Hand::Left, "thumbstick", Mode::Button);
                e.bind(i, "click", Some("/actions/global/in/crouch"));
            });
            (st, None)
        })),
        ("bind-pick-mode", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| e.popup = Some(Popup::AddMode { hand: Hand::Left, input: "thumbstick".into() }));
            (st, None)
        })),
        ("bind-pick-action", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            let index = index_of(&st, Hand::Right, "a");
            edit(&mut st, |e| e.popup = Some(Popup::Action { index, slot: "click" }));
            (st, None)
        })),
        ("bind-pick-analog", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            let index = index_of(&st, Hand::Left, "trigger");
            edit(&mut st, |e| e.popup = Some(Popup::Action { index, slot: "pull" }));
            (st, None)
        })),
        ("bind-change-mode", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            let index = index_of(&st, Hand::Right, "grip");
            edit(&mut st, |e| e.popup = Some(Popup::ChangeMode { index }));
            (st, None)
        })),
        ("bind-controller", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| e.popup = Some(Popup::Controller));
            (st, None)
        })),
        ("bind-haptics", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| e.popup = Some(Popup::Haptics));
            (st, None)
        })),
        ("bind-poses", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| e.popup = Some(Popup::Poses));
            (st, None)
        })),
        ("bind-leave", Box::new(|ctx| {
            let mut st = editing(ctx, "knuckles", knuckles(), true);
            edit(&mut st, |e| {
                let i = e.add(Hand::Left, "thumbstick", Mode::Button);
                e.bind(i, "click", Some("/actions/global/in/crouch"));
                e.popup = Some(Popup::Leave(Leave::ToList));
            });
            (st, None)
        })),
        ("bind-unreadable", Box::new(|ctx| {
            let mut doc = knuckles();
            let text = doc.to_json().replacen("\"mode\": \"button\"", "\"mode\": \"radial\"", 1);
            doc = BindingDoc::parse(&text).expect("edited sample");
            (editing(ctx, "knuckles", doc, false), None)
        })),
        ("bind-touch", Box::new(|ctx| {
            let mut st = editing(ctx, "oculus_touch", touch(), false);
            st.binds.holding = Some("oculus_touch");
            (st, None)
        })),
        ("bind-vive", Box::new(|ctx| (editing(ctx, "vive_controller", vive(), false), None))),
        // Monadeck's own: Index (trackpad) and gloves (A + B chord).
        ("bind-own", Box::new(|ctx| (own(ctx, "knuckles"), None))),
        ("bind-own-playspace", Box::new(|ctx| {
            let mut st = own(ctx, "knuckles");
            edit(&mut st, |e| e.set = own::PLAYSPACE.into());
            (st, None)
        })),
        // Nothing opens the dashboard: no saving.
        ("bind-own-nomenu", Box::new(|ctx| {
            let mut st = own(ctx, "knuckles");
            edit(&mut st, |e| e.remove(0));
            (st, None)
        })),
        ("bind-own-gloves", Box::new(|ctx| {
            let mut st = own(ctx, own::GLOVES);
            edit(&mut st, |e| e.set = own::PLAYSPACE.into());
            (st, None)
        })),
        ("bind-own-chords", Box::new(|ctx| {
            let mut st = own(ctx, own::GLOVES);
            edit(&mut st, |e| e.popup = Some(Popup::Chords));
            (st, None)
        })),
        // A game's page, with its Controls button.
        ("bind-hero", Box::new(|ctx| {
            let mut st = sample_state();
            st.games = sample_games(ctx);
            st.selected = Some(0);
            st.binds.games = sample_bindables();
            st.binds.scanned = true;
            (st, None)
        })),
    ];
    if let Ok(name) = std::env::var("MONADECK_PREVIEW_GAME") {
        shots.push(("bind-real", Box::new(move |ctx| {
            let mut st = sample_state();
            st.games = sample_games(ctx);
            st.nav = Nav::Bindings;
            let games = core::bindable_games();
            let i = games.iter().position(|g| g.name.to_lowercase().contains(&name.to_lowercase())).expect("no such game with bindings");
            let g = &games[i];
            let ty = if g.controllers.contains(&"knuckles") { "knuckles" } else { g.controllers[0] };
            let o = core::open(&g.actions_path, &g.dirs, ty).expect("open the game's binding");
            st.binds.show(games.clone(), Target::Game(i), core::controller(ty).expect("known controller"), o);
            (st, None)
        })));
    }
    for (name, setup) in shots {
        if !wanted(name) {
            continue;
        }
        let (mut st, mut pointer) = setup(ctx);
        if name == "bind-hover-spot" {
            // Where the right A button sits: measured off the layout.
            pointer = Some(right_a_spot());
        }
        // The spot hover waits a moment, then the list scrolls (animated).
        let frames = if name == "bind-hover-spot" { 40 } else { 12 };
        let img = shoot_with(ctx, textures, MAIN_PX, frames, pointer, |ctx| crate::ui::build_main(ctx, &mut st));
        let path = dir.join(format!("{name}.png"));
        img.save(&path)?;
        println!("{}", path.display());
    }
    Ok(())
}

/// The right Index drawing's A button, in points (the layout's own sums).
fn right_a_spot() -> egui::Pos2 {
    let (w, _) = (MAIN_PX.0 as f32 / crate::gfx::PPP, MAIN_PX.1 as f32 / crate::gfx::PPP);
    let body_top = 18.0 + 56.0 + 12.0 + 52.0 + 16.0;
    let cx = w / 2.0;
    let bx = egui::Rect::from_min_size(egui::pos2(cx + 8.0, body_top), egui::vec2(214.0, 300.0));
    let s = (214.0f32 / 310.0).min(300.0 / 450.0);
    let art = egui::Rect::from_center_size(bx.center(), egui::vec2(310.0 * s, 450.0 * s));
    egui::pos2(art.left() + 0.262 * art.width(), art.top() + 0.297 * art.height())
}
