//! Download + install the "built-in" runtimes: our portable Monado fork build
//! and our xrizer fork (it reloads a game's bindings while it runs, so the
//! binding editor applies at once), straight from their GitHub Releases.
//!
//! This is what powers the "no Monado found -> Install built-in" flow. It does
//! NOT build anything (that's the fork's CI); it fetches the prebuilt, portable
//! artifact, verifies it, and unpacks it into a Monadeck-owned, versioned dir so
//! it never clobbers an install the user manages themselves.
//!
//! Dependency-light by design: like the rest of core (setcap/pkexec/gpu) it
//! shells out to ubiquitous tools — `curl` (download + API), `tar`/`unzip`
//! (extract), `sha256sum` (verify) — rather than pulling an HTTP/TLS/zip stack
//! into the build. Missing tools surface as a clear error the UI can show.

use crate::paths::monadeck_data_dir;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use crate::host;

const MONADO_REPO: &str = "Eidenz/Monado";
const XRIZER_REPO: &str = "Eidenz/xrizer";
const BSB_CAMS_REPO: &str = "Eidenz/go-bsb-cams";

/// What an install produced, handed back so the caller can update config.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Installed {
    /// Release tag that was installed (e.g. `v25.1.0-eidenz1`).
    pub tag: String,
    /// Absolute path to use: the Monado prefix, or the xrizer runtime dir.
    pub path: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Is `tool` on PATH (or the common sbin dirs)?
fn have(tool: &str) -> bool {
    let on_path = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| !d.is_empty() && Path::new(d).join(tool).is_file());
    on_path || ["/usr/bin", "/usr/sbin", "/bin", "/sbin"]
        .iter()
        .any(|d| Path::new(d).join(tool).is_file())
}

fn require_tools(tools: &[&str]) -> Result<()> {
    let missing: Vec<&str> = tools.iter().copied().filter(|t| !have(t)).collect();
    if missing.is_empty() {
        Ok(())
    } else {
        bail!(
            "missing required tool(s): {} — install them and try again",
            missing.join(", ")
        )
    }
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = host::command(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch {cmd}"))?;
    if !status.success() {
        bail!("{cmd} exited with {status}");
    }
    Ok(())
}

/// Fetch the newest published (non-prerelease) release of `repo`. Bounded, so
/// a dead connection fails within seconds instead of hanging (the startup
/// update check runs it too, possibly offline).
fn latest_release(repo: &str) -> Result<Release> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let out = host::command("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "5",
            "--max-time",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: monadeck",
            &url,
        ])
        .output()
        .context("failed to launch curl")?;
    if !out.status.success() {
        bail!(
            "could not reach GitHub for {repo} (no published release yet?): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout)
        .with_context(|| format!("parsing the latest release of {repo}"))
}

impl Release {
    /// First asset whose name satisfies `pred`.
    fn asset(&self, pred: impl Fn(&str) -> bool) -> Option<&Asset> {
        self.assets.iter().find(|a| pred(&a.name))
    }
}

/// A fresh, unique scratch dir under the system temp.
fn scratch(label: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("monadeck-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).with_context(|| format!("creating scratch dir {}", dir.display()))?;
    Ok(dir)
}

fn download(url: &str, dest: &Path) -> Result<()> {
    run("curl", &["-fSL", "-o", &dest.to_string_lossy(), url])
        .with_context(|| format!("downloading {url}"))
}

/// Recursively find the directory that contains `rel` (e.g. `bin/monado-service`).
fn find_root_containing(base: &Path, rel: &str) -> Option<PathBuf> {
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir.join(rel).exists() {
            return Some(dir);
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                }
            }
        }
    }
    None
}

/// Download, verify, and unpack the latest portable Monado fork build into a
/// Monadeck-owned, versioned dir; returns its prefix.
pub fn install_monado() -> Result<Installed> {
    require_tools(&["curl", "tar", "sha256sum"])?;
    let rel = latest_release(MONADO_REPO)?;
    let tarball = rel
        .asset(|n| n.ends_with("-linux-x86_64.tar.gz"))
        .ok_or_else(|| anyhow!("no linux-x86_64 tarball in the latest {MONADO_REPO} release"))?;
    let sha = rel.asset(|n| n == format!("{}.sha256", tarball.name));

    let tmp = scratch("monado-dl")?;
    let tar_path = tmp.join(&tarball.name);
    download(&tarball.browser_download_url, &tar_path)?;

    // Verify against the published checksum when present (run from the dir so the
    // filename in the .sha256 resolves).
    if let Some(sha) = sha {
        let sha_path = tmp.join(&sha.name);
        download(&sha.browser_download_url, &sha_path)?;
        let status = host::command("sha256sum")
            .arg("-c")
            .arg(&sha.name)
            .current_dir(&tmp)
            .status()
            .context("running sha256sum")?;
        if !status.success() {
            let _ = fs::remove_dir_all(&tmp);
            bail!("checksum verification failed for {}", tarball.name);
        }
    }

    // Fresh, versioned destination; the tarball's top-level `monado/` dir is
    // stripped so the prefix holds bin/lib/share directly.
    let dest = monadeck_data_dir()
        .join("runtimes")
        .join(format!("monado-{}", rel.tag_name));
    let _ = fs::remove_dir_all(&dest);
    fs::create_dir_all(&dest)?;
    run(
        "tar",
        &[
            "-xzf",
            &tar_path.to_string_lossy(),
            "-C",
            &dest.to_string_lossy(),
            "--strip-components=1",
        ],
    )?;
    let _ = fs::remove_dir_all(&tmp);

    if !dest.join("bin/monado-service").is_file() {
        bail!(
            "extracted Monado is missing bin/monado-service at {}",
            dest.display()
        );
    }
    Ok(Installed {
        tag: rel.tag_name,
        path: dest.to_string_lossy().to_string(),
    })
}

/// Download and unpack the latest xrizer release into a Monadeck-owned dir;
/// returns the runtime dir (the one OpenVR/`VR_OVERRIDE` points at).
pub fn install_xrizer() -> Result<Installed> {
    require_tools(&["curl", "unzip"])?;
    let rel = latest_release(XRIZER_REPO)?;
    // The runtime zip is `xrizer-<tag>.zip`; skip the Windows `dependencies.zip`.
    let zip = rel
        .asset(|n| n.starts_with("xrizer") && n.ends_with(".zip"))
        .ok_or_else(|| anyhow!("no xrizer-*.zip in the latest {XRIZER_REPO} release"))?;

    let tmp = scratch("xrizer-dl")?;
    let zip_path = tmp.join(&zip.name);
    download(&zip.browser_download_url, &zip_path)?;

    let unpack = tmp.join("unpack");
    fs::create_dir_all(&unpack)?;
    run("unzip", &["-q", &zip_path.to_string_lossy(), "-d", &unpack.to_string_lossy()])?;

    // Locate the runtime root regardless of the zip's top-level dir name.
    let root = find_root_containing(&unpack, "bin/linux64/vrclient.so")
        .ok_or_else(|| anyhow!("xrizer zip did not contain bin/linux64/vrclient.so"))?;

    let dest = monadeck_data_dir()
        .join("xrizer")
        .join(&rel.tag_name);
    let _ = fs::remove_dir_all(&dest);
    fs::create_dir_all(dest.parent().expect("has parent"))?;
    fs::rename(&root, &dest)
        .or_else(|_| copy_dir(&root, &dest))
        .with_context(|| format!("installing xrizer into {}", dest.display()))?;
    let _ = fs::remove_dir_all(&tmp);

    if !dest.join("bin/linux64/vrclient.so").is_file() {
        bail!("installed xrizer is missing bin/linux64/vrclient.so");
    }
    Ok(Installed {
        tag: rel.tag_name,
        path: dest.to_string_lossy().to_string(),
    })
}

/// Download the latest `go-bsb-cams` binary (Bigscreen Beyond eye-camera server)
/// into a Monadeck-owned, versioned dir and mark it executable; returns its path.
/// A single binary asset, so no extract step.
pub fn install_bsbcams() -> Result<Installed> {
    use std::os::unix::fs::PermissionsExt;

    require_tools(&["curl"])?;
    let rel = latest_release(BSB_CAMS_REPO)?;
    let bin = rel
        .asset(|n| n == "go-bsb-cams")
        .ok_or_else(|| anyhow!("no go-bsb-cams binary in the latest {BSB_CAMS_REPO} release"))?;

    let dest_dir = monadeck_data_dir().join("bsb-cams").join(&rel.tag_name);
    let _ = fs::remove_dir_all(&dest_dir);
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join("go-bsb-cams");
    download(&bin.browser_download_url, &dest)?;

    let mut perms = fs::metadata(&dest)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dest, perms).with_context(|| format!("chmod +x {}", dest.display()))?;

    if !dest.is_file() {
        bail!("downloaded go-bsb-cams is missing at {}", dest.display());
    }
    Ok(Installed {
        tag: rel.tag_name,
        path: dest.to_string_lossy().to_string(),
    })
}

// --- Updates -------------------------------------------------------------------

/// A newer release of a runtime Monadeck installed itself.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Update {
    pub installed: String,
    pub latest: String,
}

/// Newer releases of the built-in runtimes in use, if any.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Updates {
    pub monado: Option<Update>,
    pub xrizer: Option<Update>,
}

/// The release a built-in runtime came from, read off the folder the
/// installer made (`<data>/runtimes/monado-<tag>`, `<data>/xrizer/<tag>`).
/// `None` for anything else, e.g. the user's own build: theirs to update.
fn builtin_tag(path: &Path, parent: &Path, prefix: &str) -> Option<String> {
    if path.parent()? != parent {
        return None;
    }
    let name = path.file_name()?.to_string_lossy();
    name.strip_prefix(prefix).filter(|t| !t.is_empty()).map(str::to_string)
}

pub fn builtin_monado_tag(prefix: &Path) -> Option<String> {
    builtin_tag(prefix, &monadeck_data_dir().join("runtimes"), "monado-")
}

pub fn builtin_xrizer_tag(path: &Path) -> Option<String> {
    builtin_tag(path, &monadeck_data_dir().join("xrizer"), "")
}

/// A tag's numbers, for ordering releases: `v25.1.0-eidenz10` → [25, 1, 0, 10].
fn version_key(tag: &str) -> Vec<u64> {
    tag.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn newer(installed: String, latest: String) -> Option<Update> {
    (version_key(&latest) > version_key(&installed)).then_some(Update { installed, latest })
}

/// Ask GitHub whether the built-in runtimes in use have a newer release.
/// Network; any failure (offline, rate limit) just means nothing to report.
pub fn check_updates(monado_prefix: Option<&Path>, xrizer_path: Option<&Path>) -> Updates {
    let check = |installed: Option<String>, repo: &str| {
        let installed = installed?;
        let latest = latest_release(repo).map_err(|e| log::info!("update check for {repo}: {e:#}")).ok()?;
        newer(installed, latest.tag_name)
    };
    // Side by side: offline, both time out together rather than one after the other.
    std::thread::scope(|scope| {
        let monado = scope.spawn(|| check(monado_prefix.and_then(builtin_monado_tag), MONADO_REPO));
        let xrizer = check(xrizer_path.and_then(builtin_xrizer_tag), XRIZER_REPO);
        Updates { monado: monado.join().ok().flatten(), xrizer }
    })
}

/// Recursive copy fallback for when `rename` can't cross filesystems.
fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn reads_the_tag_off_builtin_folders_only() {
        let data = monadeck_data_dir();
        let monado = data.join("runtimes").join("monado-v25.1.0-eidenz9");
        assert_eq!(builtin_monado_tag(&monado).as_deref(), Some("v25.1.0-eidenz9"));
        assert_eq!(builtin_monado_tag(Path::new("/usr")), None);
        assert_eq!(builtin_monado_tag(&data.join("runtimes").join("monado-")), None);
        let xrizer = data.join("xrizer").join("v0.5-eidenz1");
        assert_eq!(builtin_xrizer_tag(&xrizer).as_deref(), Some("v0.5-eidenz1"));
        assert_eq!(builtin_xrizer_tag(&paths_home_xrizer()), None);
    }

    fn paths_home_xrizer() -> PathBuf {
        crate::paths::home().join(".local/share/xrizer/xrizer-nightly")
    }

    #[test]
    fn only_newer_releases_count() {
        let up = |a: &str, b: &str| newer(a.into(), b.into()).is_some();
        assert!(up("v25.1.0-eidenz9", "v25.1.0-eidenz10"));
        assert!(up("v25.1.0-eidenz10", "v25.2.0-eidenz1"));
        assert!(up("v0.5-eidenz1", "v0.5-eidenz2"));
        assert!(!up("v25.1.0-eidenz10", "v25.1.0-eidenz10"));
        assert!(!up("v25.1.0-eidenz10", "v25.1.0-eidenz9"));
    }
}
