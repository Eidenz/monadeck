//! Room setup for the `steamvr_lh` tracking driver, without SteamVR.
//!
//! Monado's SteamVR Lighthouse wrapper applies SteamVR's room setup to every
//! pose: `config/chaperone_info.vrchap` holds a `standing` pose (translation +
//! yaw) per tracking universe, and the first universe listed in
//! `config/lighthouse/lighthousedb.json` that has one wins (monado
//! `steamvr_lh/device.cpp`, `load_chaperone`). Both files are plain JSON, so we
//! write the room setup ourselves instead of running SteamVR's `vrcmd`:
//!
//! 1. With Monado running, read where the headset sits on the floor through
//!    libmonado (`mnd_root_get_device_pose`, the fork's API 1.10). That pose
//!    already has the current room setup applied; undo it to get the raw
//!    tracking-universe pose.
//! 2. Pick the standing pose that puts that spot at the origin, its facing at
//!    -Z, and the floor just below the headset.
//! 3. Write it into `chaperone_info.vrchap` for the universe Monado matches.
//!    The fork re-reads the file within a second, so it applies live.
//!
//! [`crate::floor_calibration`] (SteamVR's `vrcmd`) stays as the fallback for
//! runtimes without the device-pose call.

use crate::steam;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

/// Where the headset's tracked centre sits above the floor when it stands
/// upright on it, by model. Rough figures (half the visor height); the
/// playspace height nudge covers the last centimetre.
const REST_HEIGHT_BEYOND: f64 = 0.03;
const REST_HEIGHT_DEFAULT: f64 = 0.05;

/// The headset may wobble this much (metres) while we sample it.
const STILL_TOLERANCE: f64 = 0.02;
const SAMPLES: usize = 20;
const SAMPLE_GAP: Duration = Duration::from_millis(50);

/// The fork's libmonado symbol this needs; absent on stock Monado.
const POSE_SYMBOL: &[u8] = b"mnd_root_get_device_pose";

/// One reading of the headset, in its tracking origin's space (the current
/// room setup applied, no offsets).
#[derive(Debug, Clone)]
pub struct HeadSample {
    pub position: [f64; 3],
    /// x, y, z, w.
    pub orientation: [f64; 4],
    pub tracked: bool,
    pub name: String,
}

/// SteamVR's `standing` pose for a universe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Standing {
    pub translation: [f64; 3],
    pub yaw: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomStatus {
    /// The runtime's libmonado can report the headset's pose, so the native
    /// room setup can run (needs the service up to use it).
    pub native: bool,
    /// The lighthouse driver has written its universe database.
    pub has_universe: bool,
    /// A room setup exists for a universe the driver knows.
    pub calibrated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomResult {
    pub universe: String,
    /// How far above the previous floor the headset sat (metres), when there
    /// was a room setup for this universe.
    pub previous_height: Option<f64>,
    /// How far the play space centre moved (metres), same condition.
    pub moved: Option<f64>,
    /// The running Monado picked the new room setup up (the fork re-reads it);
    /// false means it applies on the next start.
    pub applied: bool,
}

// --- Files -------------------------------------------------------------------

/// `~/.steam/root`: where monado's steamvr_lh reads SteamVR's config from.
fn steam_root() -> PathBuf {
    steam::steam_config_roots()
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from(".steam/root"))
}

fn db_path(root: &Path) -> PathBuf {
    root.join("config/lighthouse/lighthousedb.json")
}

fn chap_path(root: &Path) -> PathBuf {
    root.join("config/chaperone_info.vrchap")
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Universe ids the lighthouse driver knows, in file order.
fn known_universes(db: &Value) -> Vec<String> {
    db["known_universes"]
        .as_array()
        .map(|a| a.iter().filter_map(|u| id_string(&u["id"])).collect())
        .unwrap_or_default()
}

/// SteamVR writes ids as strings; accept numbers too.
fn id_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn standing_of(entry: &Value) -> Standing {
    let t = &entry["standing"]["translation"];
    let at = |i: usize| t.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    Standing {
        translation: [at(0), at(1), at(2)],
        yaw: entry["standing"]["yaw"].as_f64().unwrap_or(0.0),
    }
}

/// The room setup monado applies: the first known universe with an entry in
/// the chaperone file (same order monado walks them).
fn effective(universes: &[String], chap: Option<&Value>) -> Option<(String, Standing)> {
    let entries = chap?["universes"].as_array()?;
    universes.iter().find_map(|id| {
        entries
            .iter()
            .find(|e| id_string(&e["universeID"]).as_deref() == Some(id))
            .map(|e| (id.clone(), standing_of(e)))
    })
}

/// Whether the runtime at `libmonado_so` has the device-pose call. A byte scan,
/// like the xrizer live-reload probe: no need to load the library.
pub fn native_supported(libmonado_so: &Path) -> bool {
    std::fs::read(libmonado_so)
        .map(|bytes| bytes.windows(POSE_SYMBOL.len()).any(|w| w == POSE_SYMBOL))
        .unwrap_or(false)
}

/// Cheap filesystem probe. Safe to poll.
pub fn status(libmonado_so: &Path) -> RoomStatus {
    let root = steam_root();
    let universes = read_json(&db_path(&root)).map(|db| known_universes(&db)).unwrap_or_default();
    let chap = read_json(&chap_path(&root));
    RoomStatus {
        native: native_supported(libmonado_so),
        has_universe: !universes.is_empty(),
        calibrated: effective(&universes, chap.as_ref()).is_some(),
    }
}

/// `chap` with `universe`'s standing (and seated) pose set to `standing`,
/// adding the universe if it's new. Everything else is kept as it was.
fn with_standing(chap: Option<Value>, universe: &str, standing: Standing, time: &str) -> Value {
    let mut root = match chap {
        Some(Value::Object(m)) => Value::Object(m),
        _ => json!({ "jsonid": "chaperone_info", "universes": [], "version": 5 }),
    };
    if !root["universes"].is_array() {
        root["universes"] = json!([]);
    }
    let pose = json!({ "translation": standing.translation, "yaw": standing.yaw });
    let universes = root["universes"].as_array_mut().expect("universes is an array");
    let idx = universes
        .iter()
        .position(|e| id_string(&e["universeID"]).as_deref() == Some(universe));
    let entry = match idx {
        Some(i) => &mut universes[i],
        None => {
            // The same 3 × 2 m area SteamVR's quick calibration leaves behind.
            universes.push(json!({
                "collision_bounds": [
                    [[1.5, 0, 1], [1.5, 5, 1], [1.5, 5, -1], [1.5, 0, -1]],
                    [[1.5, 0, -1], [1.5, 5, -1], [-1.5, 5, -1], [-1.5, 0, -1]],
                    [[-1.5, 0, -1], [-1.5, 5, -1], [-1.5, 5, 1], [-1.5, 0, 1]],
                    [[-1.5, 0, 1], [-1.5, 5, 1], [1.5, 5, 1], [1.5, 0, 1]]
                ],
                "play_area": [3, 2],
                "universeID": universe,
            }));
            universes.last_mut().expect("just pushed")
        }
    };
    if let Value::Object(m) = entry {
        m.insert("standing".into(), pose.clone());
        m.insert("seated".into(), pose);
        m.insert("time".into(), Value::String(time.to_string()));
    }
    root
}

/// Replace `path` in one step (temp file + rename), so monado never reads a
/// half-written room setup.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("bad chaperone path")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let tmp = dir.join(".chaperone_info.vrchap.monadeck");
    std::fs::write(&tmp, text).map_err(|e| format!("Couldn't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't replace {}: {e}", path.display()))
}

/// Local time the way SteamVR stamps room setups ("Sun Jun 28 04:32:09 2026").
fn local_time_string() -> String {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        let mut buf = [0u8; 64];
        let n = libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            c"%a %b %d %H:%M:%S %Y".as_ptr(),
            &tm,
        );
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

// --- Maths -------------------------------------------------------------------
// Quaternions are [x, y, z, w]. The room setup transform is monado's: rotate
// by `yaw` about -Y, after adding `translation` (device.cpp, update_pose).

type Vec3 = [f64; 3];
type Quat = [f64; 4];

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = [q[0], q[1], q[2]];
    let t = cross(u, v).map(|c| 2.0 * c);
    let ut = cross(u, t);
    [v[0] + q[3] * t[0] + ut[0], v[1] + q[3] * t[1] + ut[1], v[2] + q[3] * t[2] + ut[2]]
}

fn mul(a: Quat, b: Quat) -> Quat {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn conjugate(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

/// The pose monado builds from a standing entry: (orientation, position).
fn room_transform(s: Standing) -> (Quat, Vec3) {
    let half = s.yaw / 2.0;
    let q = [0.0, -half.sin(), 0.0, half.cos()];
    (q, rotate(q, s.translation))
}

/// Undo a room setup: the raw tracking-universe pose behind a reported one.
fn unapply(s: Standing, position: Vec3, orientation: Quat) -> (Vec3, Quat) {
    let (q, p) = room_transform(s);
    let inv = conjugate(q);
    let local = [position[0] - p[0], position[1] - p[1], position[2] - p[2]];
    (rotate(inv, local), mul(inv, orientation))
}

/// The standing pose that puts a headset resting on the floor at the origin,
/// facing -Z, `rest_height` above the floor.
fn standing_for(raw_position: Vec3, raw_orientation: Quat, rest_height: f64) -> Result<Standing, String> {
    let forward = rotate(raw_orientation, [0.0, 0.0, -1.0]);
    if forward[0].hypot(forward[2]) < 0.5 {
        return Err("The headset isn't facing forward. Stand it upright on the floor, lenses facing \
                    the way you want to be facing, and try again."
            .into());
    }
    Ok(Standing {
        translation: [-raw_position[0], rest_height - raw_position[1], -raw_position[2]],
        yaw: (-forward[0]).atan2(-forward[2]),
    })
}

fn rest_height(name: &str) -> f64 {
    if name.to_lowercase().contains("beyond") {
        REST_HEIGHT_BEYOND
    } else {
        REST_HEIGHT_DEFAULT
    }
}

// --- The calibration ---------------------------------------------------------

/// Average a burst of headset readings, refusing if it moved or lost tracking.
fn settle(sample: &mut dyn FnMut() -> Result<HeadSample, String>) -> Result<HeadSample, String> {
    let mut readings = Vec::with_capacity(SAMPLES);
    for i in 0..SAMPLES {
        if i > 0 {
            sleep(SAMPLE_GAP);
        }
        let s = sample()?;
        if !s.tracked {
            return Err("The headset isn't tracked. Make sure the base stations can see it, then try again.".into());
        }
        readings.push(s);
    }
    let n = readings.len() as f64;
    let mut mean = [0.0; 3];
    for r in &readings {
        for (m, p) in mean.iter_mut().zip(r.position) {
            *m += p / n;
        }
    }
    let wobble = readings
        .iter()
        .map(|r| (0..3).map(|i| (r.position[i] - mean[i]).powi(2)).sum::<f64>().sqrt())
        .fold(0.0, f64::max);
    if wobble > STILL_TOLERANCE {
        return Err("The headset moved. Leave it still on the floor and try again.".into());
    }
    let mut out = readings.pop().expect("SAMPLES > 0");
    out.position = mean;
    Ok(out)
}

/// Set the floor, centre and forward direction from the headset resting on
/// the floor. `sample` reads the headset through libmonado (Monado running).
pub fn calibrate(mut sample: impl FnMut() -> Result<HeadSample, String>) -> Result<RoomResult, String> {
    let root = steam_root();
    let universes = read_json(&db_path(&root)).map(|db| known_universes(&db)).unwrap_or_default();
    if universes.is_empty() {
        return Err("The lighthouse driver hasn't found your base stations yet. Let them see the \
                    headset for a few seconds, then try again."
            .into());
    }
    let chap = read_json(&chap_path(&root));
    let current = effective(&universes, chap.as_ref());

    let head = settle(&mut sample)?;
    // What monado applies right now (nothing when no universe matched).
    let applied_now = current.as_ref().map(|(_, s)| *s).unwrap_or(Standing { translation: [0.0; 3], yaw: 0.0 });
    let (raw_pos, raw_rot) = unapply(applied_now, head.position, head.orientation);
    let rest = rest_height(&head.name);
    let standing = standing_for(raw_pos, raw_rot, rest)?;

    // Monado takes the first known universe with an entry: keep that one, or
    // add the first known universe (which then comes first).
    let universe = current.as_ref().map(|(id, _)| id.clone()).unwrap_or_else(|| universes[0].clone());
    let doc = with_standing(chap, &universe, standing, &local_time_string());
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    write_atomic(&chap_path(&root), &text)?;
    log::info!(
        "room setup: universe {universe}, standing {:?} yaw {:.3} (headset '{}', rest height {rest} m)",
        standing.translation,
        standing.yaw,
        head.name
    );

    // The fork re-reads the file once a second; see whether it did.
    sleep(Duration::from_millis(1600));
    let applied = settle(&mut sample)
        .map(|after| (after.position[1] - rest).abs() < 0.01 && after.position[0].hypot(after.position[2]) < 0.02)
        .unwrap_or(false);

    Ok(RoomResult {
        universe,
        previous_height: current.as_ref().map(|_| head.position[1]),
        moved: current.as_ref().map(|_| head.position[0].hypot(head.position[2])),
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monado's update_pose: room transform applied to a raw pose.
    fn apply(s: Standing, position: Vec3, orientation: Quat) -> (Vec3, Quat) {
        let (q, p) = room_transform(s);
        let r = rotate(q, position);
        ([r[0] + p[0], r[1] + p[1], r[2] + p[2]], mul(q, orientation))
    }

    fn yaw_quat(angle: f64) -> Quat {
        [0.0, (angle / 2.0).sin(), 0.0, (angle / 2.0).cos()]
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn unapply_inverts_apply() {
        let s = Standing { translation: [-1.246, 2.183, 2.630], yaw: 0.632 };
        let pos = [0.4, -1.9, -0.7];
        let rot = mul(yaw_quat(1.1), [0.1, 0.0, 0.0, 0.995]);
        let (dp, dr) = apply(s, pos, rot);
        let (rp, rr) = unapply(s, dp, dr);
        for i in 0..3 {
            assert!(close(rp[i], pos[i]));
        }
        for i in 0..4 {
            assert!(close(rr[i], rot[i]));
        }
    }

    #[test]
    fn calibrated_headset_sits_at_origin_facing_forward() {
        for facing in [0.0, 0.9, -2.4, 3.1] {
            let pos = [1.2, -2.15, 0.5];
            let rot = yaw_quat(facing);
            let s = standing_for(pos, rot, 0.03).unwrap();
            let (p, r) = apply(s, pos, rot);
            assert!(close(p[0], 0.0) && close(p[1], 0.03) && close(p[2], 0.0), "{p:?}");
            let f = rotate(r, [0.0, 0.0, -1.0]);
            assert!(close(f[0], 0.0) && close(f[2], -1.0), "facing {facing}: {f:?}");
        }
    }

    #[test]
    fn tilted_headset_still_finds_forward() {
        // Pitched down 30° (resting on its front edge) still has a clear heading.
        let pitch = [(-0.2618f64).sin(), 0.0, 0.0, (-0.2618f64).cos()];
        let rot = mul(yaw_quat(0.7), pitch);
        let s = standing_for([0.0, -2.0, 0.0], rot, 0.05).unwrap();
        assert!(close(s.yaw, 0.7));
        // Lenses straight down: no heading to take.
        let down = [(-0.7854f64).sin(), 0.0, 0.0, (-0.7854f64).cos()];
        assert!(standing_for([0.0, -2.0, 0.0], down, 0.05).is_err());
    }

    #[test]
    fn effective_picks_first_known_universe_with_an_entry() {
        let chap = json!({ "universes": [
            { "universeID": "222", "standing": { "translation": [1, 2, 3], "yaw": 0.5 } },
            { "universeID": "333", "standing": { "translation": [4, 5, 6], "yaw": 1.5 } }
        ]});
        let known = vec!["111".to_string(), "333".to_string(), "222".to_string()];
        let (id, s) = effective(&known, Some(&chap)).unwrap();
        assert_eq!(id, "333");
        assert_eq!(s, Standing { translation: [4.0, 5.0, 6.0], yaw: 1.5 });
        assert!(effective(&["999".to_string()], Some(&chap)).is_none());
        assert!(effective(&known, None).is_none());
    }

    #[test]
    fn known_universes_reads_string_and_number_ids() {
        let db = json!({ "known_universes": [ { "id": "1773272405" }, { "id": 42 } ] });
        assert_eq!(known_universes(&db), vec!["1773272405", "42"]);
    }

    #[test]
    fn with_standing_updates_in_place_and_keeps_the_rest() {
        let chap = json!({
            "jsonid": "chaperone_info", "version": 5,
            "universes": [
                { "universeID": "1", "play_area": [3, 2], "collision_bounds": [[[0, 0, 0]]],
                  "standing": { "translation": [0, 0, 0], "yaw": 0 }, "time": "old" },
                { "universeID": "2", "standing": { "translation": [9, 9, 9], "yaw": 9 } }
            ]
        });
        let s = Standing { translation: [1.0, 2.0, 3.0], yaw: 0.25 };
        let out = with_standing(Some(chap), "1", s, "now");
        let u = &out["universes"][0];
        assert_eq!(u["standing"]["translation"], json!([1.0, 2.0, 3.0]));
        assert_eq!(u["seated"], u["standing"]);
        assert_eq!(u["play_area"], json!([3, 2]));
        assert_eq!(u["collision_bounds"], json!([[[0, 0, 0]]]));
        assert_eq!(u["time"], "now");
        assert_eq!(out["universes"][1]["standing"]["yaw"], 9);
        assert_eq!(out["version"], 5);
    }

    #[test]
    fn with_standing_adds_a_new_universe() {
        let s = Standing { translation: [0.0, 1.0, 0.0], yaw: 0.0 };
        let out = with_standing(None, "77", s, "t");
        assert_eq!(out["jsonid"], "chaperone_info");
        let u = &out["universes"][0];
        assert_eq!(u["universeID"], "77");
        assert_eq!(u["play_area"], json!([3, 2]));
        assert_eq!(u["collision_bounds"].as_array().unwrap().len(), 4);
        // And monado would now pick it.
        let (id, back) = effective(&["77".to_string()], Some(&out)).unwrap();
        assert_eq!(id, "77");
        assert_eq!(back, s);
    }

    #[test]
    fn rest_height_by_model() {
        assert_eq!(rest_height("Bigscreen Beyond"), REST_HEIGHT_BEYOND);
        assert_eq!(rest_height("Valve Index"), REST_HEIGHT_DEFAULT);
    }
}
