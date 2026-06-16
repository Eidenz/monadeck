//! Minimal freedesktop `.desktop` support so plugins can be **installed apps**
//! (e.g. an RPM/Flatpak like NemuriXR), not just raw executable paths.
//!
//! We enumerate application entries for the picker and parse `Exec=` (field
//! codes stripped) so a desktop-entry plugin can be launched with our env, same
//! as a path plugin — no dependency on `gtk-launch`/`gio`.

use crate::paths::{config_home, home};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// An installed application, as shown in the plugin picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    /// Absolute path to the `.desktop` file (stored as the plugin's `path`).
    pub path: PathBuf,
}

struct RawEntry {
    name: Option<String>,
    exec: Option<String>,
    icon: Option<String>,
    type_: Option<String>,
    no_display: bool,
    hidden: bool,
}

fn read_entry(path: &Path) -> Option<RawEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut e = RawEntry {
        name: None,
        exec: None,
        icon: None,
        type_: None,
        no_display: false,
        hidden: false,
    };
    let mut in_main = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Only read the main group; ignore action groups etc.
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "Name" if e.name.is_none() => e.name = Some(v.to_string()),
            "Exec" if e.exec.is_none() => e.exec = Some(v.to_string()),
            "Icon" if e.icon.is_none() => e.icon = Some(v.to_string()),
            "Type" => e.type_ = Some(v.to_string()),
            "NoDisplay" => e.no_display = v.eq_ignore_ascii_case("true"),
            "Hidden" => e.hidden = v.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    Some(e)
}

/// The `Exec=` string for a `.desktop` file, if present.
pub fn entry_exec(path: &Path) -> Option<String> {
    read_entry(path).and_then(|e| e.exec)
}

/// Split an `Exec=` value into argv, honoring double-quotes and dropping the
/// freedesktop field codes (`%u`, `%F`, `%i`, …); `%%` becomes a literal `%`.
pub fn parse_exec(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_quote = !in_quote,
            '\\' if in_quote => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
        .into_iter()
        // Drop standalone field codes like %u %F %i; keep everything else.
        .filter(|t| !(t.len() == 2 && t.starts_with('%')))
        .map(|t| t.replace("%%", "%"))
        .filter(|t| !t.is_empty())
        .collect()
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"));
    dirs.push(data_home.join("applications"));
    dirs.push(data_home.join("flatpak/exports/share/applications"));

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':').filter(|s| !s.is_empty()) {
        dirs.push(PathBuf::from(d).join("applications"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    // Touch config_home so the import is used even if XDG_DATA_HOME is set;
    // some setups also drop autostart-like entries here.
    let _ = config_home();
    dirs
}

/// All launchable installed applications, deduped by desktop id (earlier/user
/// dirs win), sorted by name. NoDisplay/Hidden and non-Application entries are
/// skipped.
pub fn list_installed_apps() -> Vec<InstalledApp> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut apps = Vec::new();
    for dir in application_dirs() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Some(id) = path.file_name().and_then(|f| f.to_str()).map(String::from) else {
                continue;
            };
            if !seen.insert(id) {
                continue; // a higher-priority dir already provided this id
            }
            let Some(e) = read_entry(&path) else { continue };
            if e.hidden
                || e.no_display
                || e.exec.is_none()
                || e.type_.as_deref() != Some("Application")
            {
                continue;
            }
            if let Some(name) = e.name {
                apps.push(InstalledApp { name, path });
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

#[cfg(test)]
mod tests {
    use super::parse_exec;

    #[test]
    fn strips_field_codes_and_quotes() {
        assert_eq!(parse_exec("nemurixr %U"), vec!["nemurixr"]);
        assert_eq!(
            parse_exec("/usr/bin/foo --bar %f"),
            vec!["/usr/bin/foo", "--bar"]
        );
        assert_eq!(
            parse_exec("\"/opt/My App/run\" --x %%lit"),
            vec!["/opt/My App/run", "--x", "%lit"]
        );
        assert_eq!(
            parse_exec("flatpak run org.foo.Bar @@u %U @@"),
            vec!["flatpak", "run", "org.foo.Bar", "@@u", "@@"]
        );
    }
}
