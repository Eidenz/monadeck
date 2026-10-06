//! SteamVR's render models, read from its install: the same OBJ meshes,
//! textures and part animations SteamVR draws controllers, trackers and base
//! stations with. A model is a folder `rendermodels/<name>/` under SteamVR's
//! `resources/` or one of its drivers' (`{driver}name` names the driver), with
//! `<name>.json` listing the parts (each its own OBJ, vertices in model space)
//! plus named frames like `openxr_grip`, or a single `<name>.obj`.
//!
//! Loading happens on a worker thread; the result comes back as plain data
//! for the GPU side to upload.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use serde_json::Value;

use super::math::euler_xyz_deg;

/// One vertex as the GPU reads it: 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
}

/// The controller input a part follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Trigger,
    A,
    B,
    System,
    Stick,
}

/// How a part moves with its input (SteamVR's `motion`), in model space.
#[derive(Clone, Debug, PartialEq)]
pub enum Motion {
    /// Turns about `axis` through `pivot`, `from`..`to` degrees as the input goes 0..1.
    Rotate { input: Input, pivot: [f32; 3], axis: [f32; 3], from: f32, to: f32 },
    /// Slides `from`..`to` metres along `axis` while pressed.
    Translate { input: Input, axis: [f32; 3], from: f32, to: f32 },
    /// Tilts about `center` with the stick, and sinks by `press` when clicked.
    Joystick { center: [f32; 3], tilt: f32, press: [f32; 3] },
}

#[derive(Debug)]
pub struct Part {
    pub component: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Into [`ModelData::textures`]; None draws the tint.
    pub texture: Option<usize>,
    pub motion: Option<Motion>,
}

#[derive(Debug)]
pub struct ModelData {
    pub parts: Vec<Part>,
    pub textures: Vec<image::RgbaImage>,
    /// Frames without geometry (`openxr_grip`, `grip`, `tip`…): position + rotation.
    pub frames: HashMap<String, ([f32; 3], [f32; 4])>,
}

/// Parts SteamVR draws only in some states (touch dots, the battery display,
/// LEDs, the scroll cutaway): left out.
const SKIP: &[&str] = &["status", "led", "trackpad_touch", "trackpad_scroll_cut", "trackpad_scroll_wheel", "scroll_wheel"];

/// Every SteamVR install's root (`…/steamapps/common/SteamVR`).
pub fn steamvr_roots() -> Vec<PathBuf> {
    monadeck_core::steam::library_folders()
        .into_iter()
        .map(|lib| lib.join("steamapps/common/SteamVR"))
        .filter(|p| p.join("resources/rendermodels").is_dir())
        .collect()
}

/// The folder of model `name` (`{driver}name` or a plain name).
pub fn find(name: &str) -> Option<PathBuf> {
    let (driver, model) = match name.strip_prefix('{').and_then(|r| r.split_once('}')) {
        Some((d, m)) => (Some(d), m),
        None => (None, name),
    };
    for root in steamvr_roots() {
        let mut dirs = Vec::new();
        if let Some(d) = driver {
            dirs.push(root.join("drivers").join(d).join("resources/rendermodels"));
        }
        dirs.push(root.join("resources/rendermodels"));
        if let Ok(entries) = std::fs::read_dir(root.join("drivers")) {
            dirs.extend(entries.flatten().map(|e| e.path().join("resources/rendermodels")));
        }
        if let Some(dir) = dirs.into_iter().map(|d| d.join(model)).find(|d| d.is_dir()) {
            return Some(dir);
        }
    }
    None
}

pub fn load(name: &str) -> Result<ModelData, String> {
    let dir = find(name).ok_or_else(|| format!("no SteamVR model '{name}'"))?;
    let model = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut textures = Textures::default();
    let mut parts = Vec::new();
    let mut frames = HashMap::new();
    let json: Option<Value> = std::fs::read_to_string(dir.join(format!("{model}.json"))).ok().and_then(|t| serde_json::from_str(&t).ok());
    match json.as_ref().and_then(|j| j["components"].as_object()) {
        Some(components) => {
            for (component, c) in components {
                if let Some(local) = c["component_local"].as_object() {
                    let v3 = |k: &str| {
                        let a = local.get(k).and_then(Value::as_array);
                        let at = |i: usize| a.and_then(|a| a.get(i)).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                        [at(0), at(1), at(2)]
                    };
                    frames.insert(component.clone(), (v3("origin"), euler_xyz_deg(v3("rotate_xyz"))));
                }
                let Some(file) = c["filename"].as_str() else { continue };
                if SKIP.contains(&component.as_str()) {
                    continue;
                }
                let motion = c.get("motion").and_then(motion_of);
                for mut part in read_obj(&dir.join(file), &mut textures)? {
                    part.component = component.clone();
                    part.motion = motion.clone();
                    parts.push(part);
                }
            }
        }
        None => parts = read_obj(&dir.join(format!("{model}.obj")), &mut textures)?,
    }
    if parts.is_empty() {
        return Err(format!("SteamVR model '{name}' has no geometry"));
    }
    Ok(ModelData { parts, textures: textures.images, frames })
}

fn input_of(path: &str) -> Option<Input> {
    let p = path.to_ascii_lowercase();
    Some(if p.contains("trigger") {
        Input::Trigger
    } else if p.contains("/a/") || p.contains("/x/") || p.ends_with("/a") || p.ends_with("/x") {
        Input::A
    } else if p.contains("/b/") || p.contains("/y/") || p.ends_with("/b") || p.ends_with("/y") {
        Input::B
    } else if p.contains("system") || p.contains("menu") {
        Input::System
    } else if p.contains("joystick") || p.contains("thumbstick") {
        Input::Stick
    } else {
        return None;
    })
}

fn motion_of(m: &Value) -> Option<Motion> {
    let v3 = |k: &str| -> Option<[f32; 3]> {
        let a = m[k].as_array()?;
        Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
    };
    let range = || -> Option<(f32, f32)> {
        let a = m["value_mapping"].as_array()?;
        Some((a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
    };
    let path = ["trigger_path", "pressed_path", "component_path", "joystick_path"].iter().find_map(|k| m[*k].as_str()).unwrap_or("");
    match m["type"].as_str()? {
        "rotate" => {
            let (from, to) = range()?;
            Some(Motion::Rotate { input: input_of(path)?, pivot: v3("pivot")?, axis: v3("axis")?, from, to })
        }
        "translate" => {
            let (from, to) = range()?;
            // Some models carry a rotation's numbers here (the Index squeeze):
            // a press moves a few millimetres at most.
            if from.abs().max(to.abs()) > 0.02 {
                return None;
            }
            Some(Motion::Translate { input: input_of(path)?, axis: v3("axis")?, from, to })
        }
        "joystick" => {
            let tilt = m["joystick_rotation_x"].as_array().and_then(|a| a.first()).and_then(Value::as_f64).unwrap_or(18.0) as f32;
            Some(Motion::Joystick { center: v3("center")?, tilt, press: v3("press_translate").unwrap_or([0.0; 3]) })
        }
        _ => None,
    }
}

/// Textures by file, each loaded once.
#[derive(Default)]
struct Textures {
    images: Vec<image::RgbaImage>,
    by_path: HashMap<PathBuf, Option<usize>>,
}

impl Textures {
    fn get(&mut self, path: &Path) -> Option<usize> {
        if let Some(i) = self.by_path.get(path) {
            return *i;
        }
        let i = match image::open(path) {
            Ok(img) => {
                self.images.push(img.into_rgba8());
                Some(self.images.len() - 1)
            }
            Err(e) => {
                log::warn!("scene: texture {}: {e}", path.display());
                None
            }
        };
        self.by_path.insert(path.to_path_buf(), i);
        i
    }
}

/// `newmtl` → its `map_Kd`, from the `.mtl` files an OBJ names.
fn read_mtl(path: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut current = String::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("newmtl") => current = it.collect::<Vec<_>>().join(" "),
            Some("map_Kd") => {
                if let Some(file) = it.last() {
                    out.insert(current.clone(), dir.join(file));
                }
            }
            _ => {}
        }
    }
    out
}

/// An OBJ's triangles, one part per material (polygons fanned, missing
/// normals made flat, V flipped for Vulkan).
fn read_obj(path: &Path, textures: &mut Textures) -> Result<Vec<Part>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_obj(&text, |mtllib| read_mtl(&path.parent().unwrap_or(Path::new(".")).join(mtllib)), |file| textures.get(file))
}

fn parse_obj(
    text: &str,
    mut mtl: impl FnMut(&str) -> HashMap<String, PathBuf>,
    mut texture: impl FnMut(&Path) -> Option<usize>,
) -> Result<Vec<Part>, String> {
    let (mut pos, mut nrm, mut uv) = (Vec::<[f32; 3]>::new(), Vec::<[f32; 3]>::new(), Vec::<[f32; 2]>::new());
    let mut materials: HashMap<String, PathBuf> = HashMap::new();
    // Per material: vertices + indices, deduplicated by (v, vt, vn).
    struct Building {
        texture: Option<usize>,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        seen: HashMap<(i64, i64, i64), u32>,
    }
    let mut parts: Vec<Building> = vec![Building { texture: None, vertices: Vec::new(), indices: Vec::new(), seen: HashMap::new() }];
    let mut current = 0usize;
    let mut by_material: HashMap<String, usize> = HashMap::new();
    let floats = |it: std::str::SplitWhitespace| it.filter_map(|s| s.parse::<f32>().ok()).collect::<Vec<_>>();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let f = floats(it);
                if f.len() >= 3 {
                    pos.push([f[0], f[1], f[2]]);
                }
            }
            Some("vn") => {
                let f = floats(it);
                if f.len() >= 3 {
                    nrm.push([f[0], f[1], f[2]]);
                }
            }
            Some("vt") => {
                let f = floats(it);
                if f.len() >= 2 {
                    uv.push([f[0], 1.0 - f[1]]);
                }
            }
            Some("mtllib") => materials.extend(mtl(&it.collect::<Vec<_>>().join(" "))),
            Some("usemtl") => {
                let name = it.collect::<Vec<_>>().join(" ");
                current = *by_material.entry(name.clone()).or_insert_with(|| {
                    let tex = materials.get(&name).and_then(|p| texture(p));
                    parts.push(Building { texture: tex, vertices: Vec::new(), indices: Vec::new(), seen: HashMap::new() });
                    parts.len() - 1
                });
            }
            Some("f") => {
                let resolve = |s: &str, len: usize| -> i64 {
                    match s.parse::<i64>() {
                        Ok(i) if i < 0 => len as i64 + i,
                        Ok(i) => i - 1,
                        Err(_) => -1,
                    }
                };
                let corners: Vec<(i64, i64, i64)> = it
                    .map(|c| {
                        let mut f = c.split('/');
                        let v = resolve(f.next().unwrap_or(""), pos.len());
                        let t = resolve(f.next().unwrap_or(""), uv.len());
                        let n = resolve(f.next().unwrap_or(""), nrm.len());
                        (v, t, n)
                    })
                    .collect();
                if corners.len() < 3 || corners.iter().any(|c| c.0 < 0 || c.0 as usize >= pos.len()) {
                    continue;
                }
                let b = &mut parts[current];
                let idx = |c: (i64, i64, i64), b: &mut Building| -> u32 {
                    *b.seen.entry(c).or_insert_with(|| {
                        b.vertices.push(Vertex {
                            pos: pos[c.0 as usize],
                            nrm: nrm.get(c.2 as usize).copied().filter(|_| c.2 >= 0).unwrap_or([0.0; 3]),
                            uv: uv.get(c.1 as usize).copied().filter(|_| c.1 >= 0).unwrap_or([0.0; 2]),
                        });
                        b.vertices.len() as u32 - 1
                    })
                };
                let first = idx(corners[0], b);
                for w in corners[1..].windows(2) {
                    let (i1, i2) = (idx(w[0], b), idx(w[1], b));
                    b.indices.extend([first, i1, i2]);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for mut b in parts {
        if b.indices.is_empty() {
            continue;
        }
        flat_normals_where_missing(&mut b.vertices, &b.indices);
        out.push(Part { component: String::new(), vertices: b.vertices, indices: b.indices, texture: b.texture, motion: None });
    }
    Ok(out)
}

/// Give vertices without a normal their faces' (summed) normal.
fn flat_normals_where_missing(vertices: &mut [Vertex], indices: &[u32]) {
    if !vertices.iter().any(|v| v.nrm == [0.0; 3]) {
        return;
    }
    let mut acc = vec![[0.0f32; 3]; vertices.len()];
    for t in indices.chunks_exact(3) {
        let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
        let (pa, pb, pc) = (vertices[a].pos, vertices[b].pos, vertices[c].pos);
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for i in [a, b, c] {
            for k in 0..3 {
                acc[i][k] += n[k];
            }
        }
    }
    for (v, n) in vertices.iter_mut().zip(acc) {
        if v.nrm == [0.0; 3] {
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            v.nrm = [n[0] / l, n[1] / l, n[2] / l];
        }
    }
}

/// Loads models off the render thread: ask once, collect when ready.
pub struct Loader {
    tx: Sender<String>,
    rx: Receiver<(String, Result<ModelData, String>)>,
}

impl Loader {
    pub fn spawn() -> Self {
        let (tx, req) = mpsc::channel::<String>();
        let (res, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("scene-models".into())
            .spawn(move || {
                while let Ok(name) = req.recv() {
                    let r = load(&name);
                    if res.send((name, r)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn scene model loader");
        Self { tx, rx }
    }

    pub fn request(&self, name: &str) {
        let _ = self.tx.send(name.to_string());
    }

    pub fn try_recv(&self) -> Option<(String, Result<ModelData, String>)> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_parts_per_material_with_fans_and_flipped_v() {
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 1\nvn 0 0 1\n\
                   usemtl skin\nf 1/1/1 2/1/1 3/2/1 4/2/1\nusemtl bare\nf -4 -3 -2\n";
        let parts = parse_obj(obj, |_| HashMap::from([("skin".into(), PathBuf::from("skin.png"))]), |_| Some(7)).unwrap();
        assert_eq!(parts.len(), 2);
        // A quad fanned into two triangles, sharing corners.
        assert_eq!(parts[0].indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(parts[0].texture, Some(7));
        assert_eq!(parts[0].vertices[2].uv, [1.0, 0.0]);
        // No normals given: flat ones, facing +Z for this counter-clockwise triangle.
        assert_eq!(parts[1].texture, None);
        assert!((parts[1].vertices[0].nrm[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn motions_from_steamvr_json() {
        let trigger: Value = serde_json::from_str(r#"{"type":"rotate","trigger_path":"/input/trigger","value_mapping":[0.0,-18.0],"pivot":[0.0,-0.02,0.05],"axis":[0.977,0.119,0.179]}"#).unwrap();
        assert!(matches!(motion_of(&trigger), Some(Motion::Rotate { input: Input::Trigger, to, .. }) if to == -18.0));
        let a: Value = serde_json::from_str(r#"{"type":"translate","pressed_path":"/input/a/click","value_mapping":[0.0,0.002],"axis":[0.0,-0.927,0.375]}"#).unwrap();
        assert!(matches!(motion_of(&a), Some(Motion::Translate { input: Input::A, .. })));
        let x: Value = serde_json::from_str(r#"{"type":"translate","pressed_path":"/input/x/click","value_mapping":[-0.0005,0.0015],"axis":[0.0,-0.927,0.375]}"#).unwrap();
        assert!(matches!(motion_of(&x), Some(Motion::Translate { input: Input::A, .. })));
        // The Index squeeze's degrees in a translate: ignored.
        let squeeze: Value = serde_json::from_str(r#"{"type":"translate","component_path":"/input/grip","value_mapping":[0.0,-18.0],"axis":[0.977,0.119,0.179]}"#).unwrap();
        assert_eq!(motion_of(&squeeze), None);
    }
}
