// The OpenXR loader is built in (`openxr`'s `static` feature): nothing to find
// on the system at build or run time.

fn main() {
    bake_env("translate.env", &[("base_url", "MD_TRANSLATE_BASE_URL"), ("model", "MD_TRANSLATE_MODEL"), ("api_key", "MD_TRANSLATE_API_KEY")]);
    bake_env("picsur.env", &[("base_url", "MD_PICSUR_BASE_URL"), ("api_key", "MD_PICSUR_API_KEY")]);
}

/// Optional feature config baked in at build time from `crates/overlay/*.env`
/// (KEY=value), so endpoints/keys are never typed in VR. Absent => feature off.
fn bake_env(file: &str, keys: &[(&str, &str)]) {
    println!("cargo:rerun-if-changed={file}");
    let Ok(txt) = std::fs::read_to_string(file) else { return };
    for line in txt.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if let Some((_, env)) = keys.iter().find(|(name, _)| *name == k.trim()) {
                println!("cargo:rustc-env={env}={}", v.trim());
            }
        }
    }
}
