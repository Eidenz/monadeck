//! Download + install the "built-in" runtimes: our portable Monado fork build
//! and xrizer, straight from their GitHub Releases.
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
use std::process::Command;

const MONADO_REPO: &str = "Eidenz/Monado";
const XRIZER_REPO: &str = "Supreeeme/xrizer";
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
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch {cmd}"))?;
    if !status.success() {
        bail!("{cmd} exited with {status}");
    }
    Ok(())
}

/// Fetch the newest published (non-prerelease) release of `repo`.
fn latest_release(repo: &str) -> Result<Release> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let out = Command::new("curl")
        .args([
            "-fsSL",
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
        let status = Command::new("sha256sum")
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
