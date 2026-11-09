//! Where the app lives: installing, updating and removing it.
//!
//! **One file per OS does both install and update.**
//!
//! * **Windows** — `AkulavTracker-setup.exe` (Inno Setup, `installer/akulav-tracker.iss`). The first
//!   run asks for the install folder. Inno remembers it, so running a newer setup later, or the
//!   in-app update (which starts the setup with `/UPDATE`), installs into the same folder without
//!   asking.
//! * **Linux / macOS** — the bare binary. Started from anywhere, it offers to install itself into a
//!   folder the user picks; the folder is remembered in `<config dir>/akulav-tracker/install-dir`.
//!   A newer download started later offers to update that folder. In-app updates swap the binary
//!   in place.
//!
//! **Files go there.** The install folder holds the program *and* its data (settings, the repository
//! cache, avatars). A [`MARKER`] file identifies an install folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{bail, Context, Result};
use sha2::Digest;

use super::net;
use super::update::{UpdateInfo, CURRENT_VERSION};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg_attr(windows, allow(dead_code))]
pub const EXE_NAME: &str = if cfg!(windows) { "AkulavTracker.exe" } else { "AkulavTracker" };
/// Present in every install folder; holds the installed version.
pub const MARKER: &str = ".akulav-tracker-install";
/// Everything the app keeps in its data folder (deleted on uninstall, never anything else).
pub const DATA_ENTRIES: &[&str] = &["avatars", "cache.json", "token", "settings.json"];
const NEW_SUFFIX: &str = ".new";
const OLD_SUFFIX: &str = ".old";

// ---------------------------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------------------------

/// `AkulavTracker.exe` → `AkulavTracker.old.exe`
pub(crate) fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "AkulavTracker".into());
    let name = match exe.extension() {
        Some(ext) => format!("{stem}{suffix}.{}", ext.to_string_lossy()),
        None => format!("{stem}{suffix}"),
    };
    exe.with_file_name(name)
}

/// Folder a program belongs to: its own folder, or for `X.app/Contents/MacOS/bin` the folder that
/// holds `X.app`.
pub fn root_of_exe(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let bundle = dir
        .parent()
        .filter(|_| dir.file_name().is_some_and(|n| n == "MacOS"))
        .filter(|contents| contents.file_name().is_some_and(|n| n == "Contents"))
        .and_then(Path::parent)
        .filter(|b| b.extension().is_some_and(|e| e == "app"));
    match bundle {
        Some(b) => b.parent().map(Path::to_path_buf),
        None => Some(dir.to_path_buf()),
    }
}

/// Folder of the running program (see [`root_of_exe`]).
pub fn program_root() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| root_of_exe(&e))
}

pub fn is_install_root(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

/// Version recorded in an install folder's marker.
pub fn installed_version(root: &Path) -> Option<String> {
    let v = std::fs::read_to_string(root.join(MARKER)).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

pub(crate) fn write_marker(root: &Path) -> std::io::Result<()> {
    std::fs::write(root.join(MARKER), CURRENT_VERSION)
}

/// True when this process runs from an install folder.
pub fn running_installed() -> bool {
    program_root().is_some_and(|r| is_install_root(&r))
}

/// The install folder: the running copy's, otherwise the remembered one.
pub fn installed_root() -> Option<PathBuf> {
    program_root()
        .filter(|r| is_install_root(r))
        .or_else(|| platform::remembered_root().filter(|r| is_install_root(r)))
}

/// `portable.txt` next to the program, `--portable` or `AKULAV_PORTABLE`: keep data next to it.
pub fn portable() -> bool {
    std::env::var_os("AKULAV_PORTABLE").is_some() || program_root().is_some_and(|r| r.join("portable.txt").exists())
}

/// The data folder: `AKULAV_DATA_DIR`; portable → next to the program; installed → the install
/// folder; otherwise (development builds, "just run it") the user's data folder.
pub fn data_root() -> PathBuf {
    if let Some(d) = std::env::var_os("AKULAV_DATA_DIR") {
        return PathBuf::from(d);
    }
    if portable() {
        if let Some(r) = program_root() {
            return r;
        }
    }
    if let Some(r) = program_root().filter(|r| is_install_root(r)) {
        return r;
    }
    dirs::data_dir().or_else(dirs::home_dir).unwrap_or_else(std::env::temp_dir).join("AkulavTracker")
}

#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (canon(a), canon(b));
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Check a folder typed into the install dialog. `~` expands to the home folder.
#[cfg_attr(windows, allow(dead_code))]
pub fn parse_install_dir(text: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Choose a folder.".into());
    }
    let path = match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            dirs::home_dir().ok_or("Could not find your home folder.")?.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(text),
    };
    if !path.is_absolute() {
        return Err("Use a full path, for example ~/Apps/AkulavTracker.".into());
    }
    if path.parent().is_none() {
        return Err("Pick a folder, not the root of a drive.".into());
    }
    if path.is_file() {
        return Err("That's a file, not a folder.".into());
    }
    Ok(path)
}

// ---------------------------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------------------------

/// Installing offered on Linux/macOS when the binary runs from outside its install folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
pub enum Offer {
    #[default]
    None,
    /// Not installed yet: pick a folder.
    Install { suggested: PathBuf },
    /// An older copy is installed in `root` (the remembered folder).
    Update { root: PathBuf, installed: Option<String> },
}

/// Passed from `main` to the UI.
#[derive(Debug, Clone, Default)]
pub struct StartupInfo {
    /// Started right after a fresh install.
    pub installed: bool,
    pub updated_from: Option<String>,
    pub offer: Offer,
}

pub enum Startup {
    Run(StartupInfo),
    Exit,
}

pub fn on_startup(args: &[String]) -> Startup {
    let flag = |f: &str| args.iter().any(|a| a == f);
    if flag("--portable") {
        std::env::set_var("AKULAV_PORTABLE", "1");
    }
    let Ok(exe) = std::env::current_exe() else { return Startup::Run(StartupInfo::default()) };

    // Leftovers from the previous update.
    let _ = std::fs::remove_file(sibling(&exe, OLD_SUFFIX));
    let _ = std::fs::remove_file(sibling(&exe, NEW_SUFFIX));

    if flag("--uninstall") {
        platform::uninstall_interactive();
        return Startup::Exit;
    }

    let mut info = StartupInfo {
        installed: flag("--installed"),
        updated_from: args
            .iter()
            .position(|a| a == "--updated-from")
            .and_then(|i| args.get(i + 1))
            .filter(|v| !v.trim().is_empty() && !v.starts_with("--"))
            .cloned(),
        ..Default::default()
    };

    if let Some(root) = program_root().filter(|r| is_install_root(r)) {
        platform::on_installed_start(&root);
    } else if !cfg!(debug_assertions) && !flag("--here") && !portable() {
        info.offer = platform::startup_offer();
    }
    Startup::Run(info)
}

// ---------------------------------------------------------------------------------------------
// Installing (Linux/macOS) and uninstalling
// ---------------------------------------------------------------------------------------------

/// Install this binary into `root` (or update the copy there), then start it with `args`.
/// On success the caller must exit. Windows installs with the setup instead.
#[cfg_attr(windows, allow(dead_code))]
pub fn install_and_launch(root: &Path, args: &[&str]) -> Result<()> {
    platform::install_and_launch(root, args)
}

/// Whether the UI asks about deleting data itself (Windows' uninstaller asks on its own).
pub const UNINSTALL_ASKS_ABOUT_DATA: bool = cfg!(windows);

/// Uninstall the running installed copy. On success the caller must exit.
pub fn uninstall(delete_data: bool) -> Result<()> {
    platform::uninstall(delete_data)
}

/// Delete [`DATA_ENTRIES`] in `dir` (never anything else), then `dir` itself if it is empty.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn delete_data(dir: &Path) {
    for name in DATA_ENTRIES {
        let p = dir.join(name);
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(&p);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
    let _ = std::fs::remove_dir(dir);
}

// ---------------------------------------------------------------------------------------------
// Updating
// ---------------------------------------------------------------------------------------------

fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut hasher = sha2::Sha256::new();
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Replace `target` with `source` even if `target` is the running executable (both OSes allow
/// renaming a running program). The previous file stays as `.old` until the next start.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn swap_in(source: &Path, target: &Path) -> Result<()> {
    let old = sibling(target, OLD_SUFFIX);
    let _ = std::fs::remove_file(&old);
    let had_target = target.exists();
    if had_target {
        std::fs::rename(target, &old).with_context(|| format!("could not move {} aside", target.display()))?;
    }
    if let Err(e) = std::fs::rename(source, target).or_else(|_| std::fs::copy(source, target).map(|_| ())) {
        if had_target {
            let _ = std::fs::rename(&old, target); // roll back
        }
        return Err(e).with_context(|| format!("could not write {}", target.display()));
    }
    let _ = std::fs::remove_file(source);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

fn looks_like_executable(bytes: &[u8]) -> bool {
    if cfg!(windows) {
        bytes.starts_with(b"MZ")
    } else if cfg!(target_os = "macos") {
        bytes.len() >= 4 && matches!(&bytes[..4], [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe])
    } else {
        bytes.starts_with(b"\x7fELF")
    }
}

/// Reject anything that isn't a program for this OS or doesn't match its published SHA-256.
pub fn verify_update(file: &Path, sha256: &str) -> Result<()> {
    let mut head = [0u8; 4];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(file)?;
        let _ = f.read(&mut head)?;
    }
    if !looks_like_executable(&head) {
        let _ = std::fs::remove_file(file);
        bail!("the download is not a program for this system");
    }
    if !sha256_file(file)?.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(file);
        bail!("the download did not match its SHA-256 checksum, so it was deleted");
    }
    Ok(())
}

/// Download and verify this OS's release file. `on_bytes(done, total)` reports progress.
pub async fn download_update(
    client: &reqwest::Client,
    info: &UpdateInfo,
    on_bytes: &(dyn Fn(u64, u64) + Send + Sync),
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let staged = platform::staging_path()?;
    let _ = std::fs::remove_file(&staged);
    let total = client.head(&info.url).send().await.ok().and_then(|r| r.content_length()).unwrap_or(0);
    let cb = |done: u64| on_bytes(done, total.max(done));
    net::download_file(client, &info.url, &staged, &cb, cancel).await?;
    verify_update(&staged, &info.sha256)?;
    Ok(staged)
}

/// Apply a verified download: Windows runs the setup in update mode (same folder), Linux/macOS
/// swap the binary in place. Either way the app restarts; the caller must exit right away.
pub fn apply_update(staged: &Path) -> Result<()> {
    platform::apply_update(staged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_names() {
        assert_eq!(sibling(Path::new("/x/AkulavTracker.exe"), ".old"), PathBuf::from("/x/AkulavTracker.old.exe"));
        assert_eq!(sibling(Path::new("/x/AkulavTracker"), ".new"), PathBuf::from("/x/AkulavTracker.new"));
    }

    #[test]
    fn program_roots() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(root_of_exe(&p("/opt/akulav/AkulavTracker")), Some(p("/opt/akulav")));
        assert_eq!(
            root_of_exe(&p("/Users/a/Applications/Akulav Tracker/Akulav Tracker.app/Contents/MacOS/AkulavTracker")),
            Some(p("/Users/a/Applications/Akulav Tracker"))
        );
        assert_eq!(root_of_exe(&p("/x/MacOS/AkulavTracker")), Some(p("/x/MacOS")));
    }

    #[test]
    fn swap_replaces_and_keeps_old() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("app.exe");
        let staged = d.path().join("app.new.exe");
        std::fs::write(&target, b"v1").unwrap();
        std::fs::write(&staged, b"v2").unwrap();
        swap_in(&staged, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"v2");
        assert_eq!(std::fs::read(d.path().join("app.old.exe")).unwrap(), b"v1");
        assert!(!staged.exists());
    }

    #[test]
    fn verifies_downloads() {
        let d = tempfile::tempdir().unwrap();
        let bad = d.path().join("bad");
        std::fs::write(&bad, b"<html>404</html>").unwrap();
        assert!(verify_update(&bad, "00").is_err());
        assert!(!bad.exists(), "rejected downloads are deleted");

        let header: &[u8] = if cfg!(windows) {
            b"MZ\x90\x00"
        } else if cfg!(target_os = "macos") {
            &[0xcf, 0xfa, 0xed, 0xfe]
        } else {
            b"\x7fELF"
        };
        let good = d.path().join("good");
        std::fs::write(&good, header).unwrap();
        let sum = sha256_file(&good).unwrap();
        assert!(verify_update(&good, &sum.to_uppercase()).is_ok());
        assert!(verify_update(&good, &"0".repeat(64)).is_err());
        assert!(!good.exists());
    }

    #[test]
    fn deletes_only_app_data() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Apps");
        std::fs::create_dir_all(root.join("avatars/x")).unwrap();
        std::fs::write(root.join("settings.json"), b"{}").unwrap();
        std::fs::write(root.join("my-notes.txt"), b"mine").unwrap();
        delete_data(&root);
        assert!(!root.join("avatars").exists() && !root.join("settings.json").exists());
        assert!(root.join("my-notes.txt").is_file());
    }

    #[test]
    fn install_dir_input() {
        assert!(parse_install_dir("").is_err());
        assert!(parse_install_dir("relative/dir").is_err());
        if cfg!(unix) {
            assert!(parse_install_dir("/").is_err());
            assert_eq!(parse_install_dir(" /opt/akulav ").unwrap(), PathBuf::from("/opt/akulav"));
            assert!(parse_install_dir("~/Apps/Akulav").unwrap().ends_with("Apps/Akulav"));
        } else {
            assert!(parse_install_dir(r"C:\").is_err());
            assert_eq!(parse_install_dir(r"D:\Apps\Akulav").unwrap(), PathBuf::from(r"D:\Apps\Akulav"));
        }
    }
}
