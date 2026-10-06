//! Self-update from GitHub Releases: find the latest release, download the file for
//! this platform, check it against the release's `SHA256SUMS.txt`, swap it in, and
//! relaunch.
//!
//! - macOS: the `.dmg`; its `dust.app` replaces the running bundle.
//! - Windows / Linux: the bare binary (`dust-<v>-windows-x64.exe`,
//!   `dust-<v>-linux-<arch>`); the running executable is renamed aside, which both
//!   systems allow, and the new one takes its place.
//!
//! Where dust can't replace itself (a dev build, a read-only location, a release
//! without a matching file) the caller opens the release page instead.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const LATEST: &str = "https://api.github.com/repos/domknez/dust/releases/latest";
const SUMS: &str = "SHA256SUMS.txt";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    /// The release page, for manual installs.
    pub page: String,
    assets: Vec<(String, String)>,
}

impl Release {
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
    /// macOS app bundle.
    Bundle(PathBuf),
    /// Executable file (Windows, Linux).
    Binary(PathBuf),
}

/// What to start once the new version is in place.
pub struct Restart(PathBuf);

fn package_name(version: &str) -> Option<String> {
    if cfg!(target_os = "macos") {
        Some(format!("dust-{version}-macos.dmg"))
    } else if cfg!(windows) {
        Some(format!("dust-{version}-windows-x64.exe"))
    } else if cfg!(target_os = "linux") {
        Some(format!("dust-{version}-linux-{}", std::env::consts::ARCH))
    } else {
        None
    }
}

fn agent(timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("dust/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(timeout)
        .build()
        .into()
}

/// `a` is a newer `major.minor.patch` than `b`.
pub fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(a) > parse(b)
}

/// The latest release, if it is newer than this build.
pub fn check() -> Result<Option<Release>, String> {
    let reply: serde_json::Value = agent(Some(Duration::from_secs(20)))
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .into_body()
        .read_json()
        .map_err(|e| e.to_string())?;
    let version = reply["tag_name"].as_str().ok_or("no tag in release")?.trim_start_matches('v').to_string();
    let assets = reply["assets"]
        .as_array()
        .map(|a| {
            a.iter().filter_map(|x| Some((x["name"].as_str()?.to_string(), x["browser_download_url"].as_str()?.to_string()))).collect()
        })
        .unwrap_or_default();
    let page = reply["html_url"].as_str().unwrap_or_default().to_string();
    let release = Release { version, page, assets };
    Ok(is_newer(&release.version, CURRENT).then_some(release))
}

/// Whether [`install`] can replace this copy with `release` (otherwise: open the page).
pub fn can_install(release: &Release) -> bool {
    release.package().is_some() && release.asset(SUMS).is_some() && target().is_some_and(|t| writable(&t))
}

fn target() -> Option<Target> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if cfg!(target_os = "macos") {
        // .../dust.app/Contents/MacOS/dust; a bare binary is a dev build.
        let bundle = exe.ancestors().nth(3)?;
        (bundle.extension()? == "app").then(|| Target::Bundle(bundle.to_path_buf()))
    } else {
        Some(Target::Binary(exe))
    }
}

/// The replacement is renamed into place, so its folder must be writable.
fn writable(target: &Target) -> bool {
    let (Target::Bundle(p) | Target::Binary(p)) = target;
    let Some(dir) = p.parent() else { return false };
    let probe = dir.join(".dust-write-test");
    let ok = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// A downloaded, verified update, unpacked next to the running copy and waiting
/// for [`apply`].
pub struct Staged {
    pub version: String,
    target: Target,
    path: PathBuf,
}

/// Download and verify `release`, then unpack it next to the running copy so that
/// [`apply`] only has to rename. `progress` goes 0..=1000 during the download.
pub fn stage(release: &Release, progress: &Arc<AtomicU32>) -> Result<Staged, String> {
    let target = target().ok_or("this copy of dust can't update itself")?;
    let (name, url) = release.package().ok_or("the release has no download for this platform")?;
    let sums = agent(Some(Duration::from_secs(20)))
        .get(release.asset(SUMS).ok_or("the release has no checksums")?)
        .call()
        .and_then(|r| r.into_body().read_to_string())
        .map_err(|e| format!("checksums: {e}"))?;
    let expected = expected_sha256(&sums, &name).ok_or_else(|| format!("no checksum for {name}"))?;

    let dir = std::env::temp_dir().join(format!("dust-update-{}", release.version));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(&name);
    let actual = download(url, &file, progress)?;
    if actual != expected {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("{name}: checksum mismatch"));
    }
    log_info!("update: downloaded and verified {name}");

    let path = staged_path(&target);
    let unpacked = match &target {
        Target::Bundle(_) => unpack_bundle(&file, &path, &dir),
        Target::Binary(_) => stage_binary(&file, &path),
    };
    let _ = std::fs::remove_dir_all(&dir);
    unpacked?;
    Ok(Staged { version: release.version.clone(), target, path })
}

/// Swap the staged copy in place of the running one. Quick (renames only), so it
/// can run right before quitting. Returns what [`relaunch`] should start.
pub fn apply(staged: &Staged) -> Result<Restart, String> {
    let (Target::Bundle(current) | Target::Binary(current)) = &staged.target;
    let old = old_path(&staged.target);
    let _ = remove(&old);
    std::fs::rename(current, &old).map_err(|e| format!("moving the old version aside: {e}"))?;
    if let Err(e) = std::fs::rename(&staged.path, current) {
        let _ = std::fs::rename(&old, current);
        return Err(format!("installing the new version: {e}"));
    }
    // A running Windows executable can't be deleted; `clean_up` gets it next time.
    let _ = remove(&old);
    log_info!("update: installed {}", staged.version);
    Ok(Restart(current.clone()))
}

/// [`stage`] then [`apply`] in one go (command line).
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
    let resp = agent(Some(Duration::from_secs(600))).get(url).call().map_err(|e| format!("download: {e}"))?;
    let total = resp.body().content_length().unwrap_or(0);
    let mut body = resp.into_body().into_reader();
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

/// macOS: copy `dust.app` out of the DMG to `dest`, next to the running bundle.
fn unpack_bundle(dmg: &Path, dest: &Path, work: &Path) -> Result<(), String> {
    let mount = work.join("mnt");
    run("hdiutil", &["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint", path_str(&mount)?, path_str(dmg)?])?;
    let _ = std::fs::remove_dir_all(dest);
    let copied = run("ditto", &[path_str(&mount.join("dust.app"))?, path_str(dest)?]);
    let _ = run("hdiutil", &["detach", "-quiet", path_str(&mount)?]);
    copied
}

/// Windows / Linux: the new executable next to the running one (which can be
/// renamed but not overwritten while it runs).
fn stage_binary(new: &Path, dest: &Path) -> Result<(), String> {
    std::fs::copy(new, dest).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn staged_path(target: &Target) -> PathBuf {
    match target {
        Target::Bundle(b) => b.with_file_name(".dust-update.app"),
        Target::Binary(exe) => exe.with_extension("new"),
    }
}

fn old_path(target: &Target) -> PathBuf {
    match target {
        Target::Bundle(b) => b.with_file_name(".dust-old.app"),
        Target::Binary(exe) => exe.with_extension("old"),
    }
}

fn remove(path: &Path) -> std::io::Result<()> {
    if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
}

/// Remove what an earlier session left behind: the old version (Windows can't
/// delete a running exe) and a staged update that was never applied.
pub fn clean_up() {
    if let Some(target) = target() {
        let _ = remove(&old_path(&target));
        let _ = remove(&staged_path(&target));
    }
}

/// Start the new version once this process has exited. Call just before quitting.
pub fn relaunch(restart: &Restart) {
    let path = &restart.0;
    let spawned = if cfg!(target_os = "macos") {
        // `open` would just focus the old instance while it still runs.
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
    fn versions_compare_numerically() {
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(is_newer("v1.0.0", "0.99.0"));
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }

    #[test]
    fn checksum_lines() {
        let sums = "abc123  dust-0.3.0-macos.dmg\nDEF456  dust-0.3.0-windows-x64.exe\n";
        assert_eq!(expected_sha256(sums, "dust-0.3.0-windows-x64.exe").as_deref(), Some("def456"));
        assert_eq!(expected_sha256(sums, "dust-0.3.0-macos.dmg").as_deref(), Some("abc123"));
        assert_eq!(expected_sha256(sums, "dust-0.3.0-linux-x86_64"), None);
    }

    #[test]
    fn package_per_platform() {
        let release = Release {
            version: "0.3.0".into(),
            page: String::new(),
            assets: vec![("dust-0.3.0-macos.dmg".into(), "u1".into()), ("dust-0.3.0-windows-x64.exe".into(), "u2".into())],
        };
        if cfg!(target_os = "macos") {
            assert_eq!(release.package(), Some(("dust-0.3.0-macos.dmg".into(), "u1")));
        } else if cfg!(windows) {
            assert_eq!(release.package(), Some(("dust-0.3.0-windows-x64.exe".into(), "u2")));
        } else {
            assert_eq!(release.package(), None);
        }
    }
}
