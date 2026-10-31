//! Updating drift from its GitHub releases: find the latest release, download the
//! file for this platform, check it against the release's `SHA256SUMS.txt`, swap it
//! in, and relaunch.
//!
//! - macOS: the `.zip` with `drift.app`, which replaces the running bundle.
//! - Linux and Windows: the archive's `drift` (`drift.exe`), which takes the place of
//!   the running executable (renamed aside first; both systems allow that while it
//!   runs).
//!
//! Where drift can't replace itself (a build of your own, a read-only folder, a
//! release without a matching file) the caller opens the release page instead.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// This build's version.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Where drift's releases are published.
const LATEST: &str = "https://api.github.com/repos/domknez/drift/releases/latest";
const SUMS: &str = "SHA256SUMS.txt";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    /// The release page, for installing by hand.
    pub page: String,
    /// Each file's name and download address.
    assets: Vec<(String, String)>,
}

impl Release {
    /// A made-up release, for the demo.
    pub fn demo(version: &str) -> Release {
        Release { version: version.to_string(), page: "https://github.com/domknez/drift/releases".into(), assets: Vec::new() }
    }

    fn asset(&self, name: &str) -> Option<&str> {
        self.assets.iter().find(|(n, _)| n == name).map(|(_, url)| url.as_str())
    }

    /// The file this platform installs from, if the release has it.
    fn package(&self) -> Option<(String, &str)> {
        let name = package_name(&self.version)?;
        let url = self.asset(&name)?;
        Some((name, url))
    }
}

/// Where the running copy lives and how it gets replaced.
enum Target {
    /// The macOS app bundle.
    Bundle(PathBuf),
    /// The executable (Linux, Windows).
    Binary(PathBuf),
}

impl Target {
    fn path(&self) -> &Path {
        let (Target::Bundle(p) | Target::Binary(p)) = self;
        p
    }
}

/// What to start once the new version is in place.
pub struct Restart(PathBuf);

/// The release file for this platform, as `release.yml` names it.
fn package_name(version: &str) -> Option<String> {
    let arch = std::env::consts::ARCH;
    if cfg!(target_os = "macos") {
        Some(format!("drift-{version}-macos-universal.zip"))
    } else if cfg!(windows) {
        Some(format!("drift-{version}-windows-{arch}.zip"))
    } else if cfg!(target_os = "linux") {
        Some(format!("drift-{version}-linux-{arch}.tar.gz"))
    } else {
        None
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("drift/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Whether version `a` (`v1.2.3` or `1.2.3`) is newer than `b`.
pub fn newer(a: &str, b: &str) -> bool {
    let parts =
        |v: &str| -> Vec<u64> { v.trim_start_matches('v').split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

/// The latest release, if it is newer than this build. Blocks; run it off the UI
/// thread.
pub fn check() -> Result<Option<Release>, String> {
    let response =
        agent(Duration::from_secs(20)).get(LATEST).header("Accept", "application/vnd.github+json").call().map_err(|e| e.to_string())?;
    match response.status().as_u16() {
        // Nothing released yet.
        404 => return Ok(None),
        200..=299 => {}
        status => return Err(format!("GitHub answered {status}")),
    }
    let reply: serde_json::Value = response.into_body().read_json().map_err(|e| e.to_string())?;
    let version = reply["tag_name"].as_str().ok_or("no version in the answer")?.trim_start_matches('v').to_string();
    let assets = reply["assets"]
        .as_array()
        .map(|a| {
            a.iter().filter_map(|x| Some((x["name"].as_str()?.to_string(), x["browser_download_url"].as_str()?.to_string()))).collect()
        })
        .unwrap_or_default();
    let page = reply["html_url"].as_str().unwrap_or("https://github.com/domknez/drift/releases").to_string();
    let release = Release { version, page, assets };
    Ok(newer(&release.version, CURRENT).then_some(release))
}

/// Whether [`stage`] can replace this copy with `release` (otherwise: open the page).
pub fn can_install(release: &Release) -> bool {
    release.package().is_some() && release.asset(SUMS).is_some() && target().is_some_and(|t| writable(&t))
}

fn target() -> Option<Target> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if cfg!(target_os = "macos") {
        // .../drift.app/Contents/MacOS/drift; a bare binary is a build of your own.
        let bundle = exe.ancestors().nth(3)?;
        (bundle.extension()? == "app").then(|| Target::Bundle(bundle.to_path_buf()))
    } else {
        Some(Target::Binary(exe))
    }
}

/// The replacement is renamed into place, so its folder must be writable.
fn writable(target: &Target) -> bool {
    let Some(dir) = target.path().parent() else { return false };
    let probe = dir.join(".drift-write-test");
    let ok = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// A downloaded, checked update, unpacked next to the running copy and waiting for
/// [`apply`].
pub struct Staged {
    pub version: String,
    target: Target,
    path: PathBuf,
}

/// Download and check `release`, then unpack it next to the running copy so that
/// [`apply`] only has to rename. `progress` goes 0..=1000 during the download.
pub fn stage(release: &Release, progress: &Arc<AtomicU32>) -> Result<Staged, String> {
    let target = target().ok_or("this copy of drift can't update itself")?;
    let (name, url) = release.package().ok_or("the release has no download for this platform")?;
    let sums = agent(Duration::from_secs(20))
        .get(release.asset(SUMS).ok_or("the release has no checksums")?)
        .call()
        .and_then(|r| r.into_body().read_to_string())
        .map_err(|e| format!("checksums: {e}"))?;
    let expected = expected_sha256(&sums, &name).ok_or_else(|| format!("no checksum for {name}"))?;

    let work = std::env::temp_dir().join(format!("drift-update-{}", release.version));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let file = work.join(&name);
    let actual = download(url, &file, progress)?;
    if actual != expected {
        let _ = std::fs::remove_dir_all(&work);
        return Err(format!("{name}: checksum mismatch"));
    }
    log_info!("update: downloaded and checked {name}");

    let path = staged_path(&target);
    let unpacked = match &target {
        Target::Bundle(_) => unpack_bundle(&file, &path, &work),
        Target::Binary(_) => unpack_binary(&file, &path, &work),
    };
    let _ = std::fs::remove_dir_all(&work);
    unpacked?;
    Ok(Staged { version: release.version.clone(), target, path })
}

/// Swap the staged copy in place of the running one. Quick (renames only), so it can
/// run right before quitting. Returns what [`relaunch`] should start.
pub fn apply(staged: &Staged) -> Result<Restart, String> {
    let current = staged.target.path();
    let old = old_path(&staged.target);
    let _ = remove(&old);
    std::fs::rename(current, &old).map_err(|e| format!("moving the old version aside: {e}"))?;
    if let Err(e) = std::fs::rename(&staged.path, current) {
        let _ = std::fs::rename(&old, current);
        return Err(format!("installing the new version: {e}"));
    }
    // Windows can't delete a running executable; `clean_up` gets it next time.
    let _ = remove(&old);
    log_info!("update: installed {}", staged.version);
    Ok(Restart(current.to_path_buf()))
}

/// [`stage`] then [`apply`] in one go (`drift update`).
pub fn install(release: &Release, progress: &Arc<AtomicU32>) -> Result<Restart, String> {
    apply(&stage(release, progress)?)
}

/// `sha256sum` output: `<hex>  <name>` per line.
fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.to_ascii_lowercase())
    })
}

/// Stream `url` into `path`; returns the file's SHA-256 (hex).
fn download(url: &str, path: &Path, progress: &Arc<AtomicU32>) -> Result<String, String> {
    let response = agent(Duration::from_secs(600)).get(url).call().map_err(|e| format!("download: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("download: GitHub answered {}", response.status().as_u16()));
    }
    let total = response.body().content_length().unwrap_or(0);
    let mut body = response.into_body().into_reader();
    let mut out = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let (mut buf, mut done) = (vec![0u8; 64 * 1024], 0u64);
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("download: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if let Some(permille) = (done * 1000).checked_div(total) {
            progress.store(permille.min(1000) as u32, Ordering::Relaxed);
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// macOS: `drift.app` out of the zip, to `dest` next to the running bundle.
fn unpack_bundle(zip: &Path, dest: &Path, work: &Path) -> Result<(), String> {
    let unpacked = work.join("unpacked");
    run("ditto", &["-x", "-k", path_str(zip)?, path_str(&unpacked)?])?;
    let app = unpacked.join("drift.app");
    if !app.is_dir() {
        return Err("the download has no drift.app".into());
    }
    let _ = std::fs::remove_dir_all(dest);
    run("ditto", &[path_str(&app)?, path_str(dest)?])
}

/// Linux and Windows: the archive's executable (in its one folder,
/// `drift-<v>-<platform>/`), to `dest` next to the running one. `tar` reads both the
/// `.tar.gz` and, on Windows (10 and later), the `.zip`.
fn unpack_binary(archive: &Path, dest: &Path, work: &Path) -> Result<(), String> {
    let unpacked = work.join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    run("tar", &["-xf", path_str(archive)?, "-C", path_str(&unpacked)?])?;
    let exe = if cfg!(windows) { "drift.exe" } else { "drift" };
    let new = std::fs::read_dir(&unpacked)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path().join(exe))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("the download has no {exe}"))?;
    std::fs::copy(&new, dest).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn staged_path(target: &Target) -> PathBuf {
    match target {
        Target::Bundle(bundle) => bundle.with_file_name(".drift-update.app"),
        Target::Binary(exe) => with_suffix(exe, ".new"),
    }
}

fn old_path(target: &Target) -> PathBuf {
    match target {
        Target::Bundle(bundle) => bundle.with_file_name(".drift-old.app"),
        Target::Binary(exe) => with_suffix(exe, ".old"),
    }
}

/// `drift.exe` → `drift.exe.old`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove(path: &Path) -> std::io::Result<()> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
}

/// Remove what an earlier run left behind: the old version (Windows can't delete a
/// running executable) and an update that was staged but never applied.
pub fn clean_up() {
    let Some(target) = target() else { return };
    let _ = remove(&staged_path(&target));
    let _ = remove(&old_path(&target));
}

/// Start the new version once this process has exited. Call just before quitting.
pub fn relaunch(restart: &Restart) {
    let path = &restart.0;
    let spawned = if cfg!(target_os = "macos") {
        // `open` would only bring the old drift forward while it still runs.
        let script = format!("while kill -0 {} 2>/dev/null; do sleep 0.2; done; open \"$0\"", std::process::id());
        std::process::Command::new("/bin/sh").arg("-c").arg(script).arg(path).spawn()
    } else {
        std::process::Command::new(path).spawn()
    };
    if let Err(e) = spawned {
        log_warn!("update: relaunch failed: {e}");
    }
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("{program}: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

fn path_str(p: &Path) -> Result<&str, String> {
    p.to_str().ok_or_else(|| format!("unsupported path {}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("0.1.2", "0.1.1"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn reads_checksum_lines() {
        let sums = "abc123  drift-0.2.0-macos-universal.zip\nDEF456  drift-0.2.0-windows-x86_64.zip\n";
        assert_eq!(expected_sha256(sums, "drift-0.2.0-windows-x86_64.zip").as_deref(), Some("def456"));
        assert_eq!(expected_sha256(sums, "drift-0.2.0-macos-universal.zip").as_deref(), Some("abc123"));
        assert_eq!(expected_sha256(sums, "drift-0.2.0-linux-x86_64.tar.gz"), None);
    }

    #[test]
    fn picks_this_platforms_file() {
        let arch = std::env::consts::ARCH;
        let names = [
            "drift-0.2.0-macos-universal.zip".to_string(),
            format!("drift-0.2.0-windows-{arch}.zip"),
            format!("drift-0.2.0-linux-{arch}.tar.gz"),
        ];
        let release = Release {
            version: "0.2.0".into(),
            page: String::new(),
            assets: names.iter().enumerate().map(|(i, n)| (n.clone(), format!("u{i}"))).collect(),
        };
        let i = if cfg!(target_os = "macos") {
            0
        } else if cfg!(windows) {
            1
        } else {
            2
        };
        assert_eq!(release.package(), Some((names[i].clone(), format!("u{i}").as_str())));
    }
}
