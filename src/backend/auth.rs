//! Signing in to GitHub and keeping the token.
//!
//! Three ways in:
//! * **Sign in with GitHub** — the OAuth device flow (no password ever touches this app). Needs the
//!   client ID of a GitHub OAuth App with "Device flow" enabled, baked in at build time through
//!   `AKULAV_GITHUB_CLIENT_ID` (or set in the environment at run time).
//! * **GitHub CLI** — reuse the token `gh auth token` prints, when `gh` is installed and signed in.
//! * **Personal access token** — pasted by the user.
//!
//! The token is kept in the OS credential store (Windows Credential Manager, macOS Keychain). On
//! Linux, and if the store is unavailable, it goes into `<data folder>/token` (mode 0600).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Scopes requested by the device flow: private repos (for their statistics) and org membership.
pub const SCOPES: &str = "repo read:user read:org";

pub fn client_id() -> Option<String> {
    std::env::var("AKULAV_GITHUB_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("AKULAV_GITHUB_CLIENT_ID").map(str::to_string))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub async fn start_device_flow(http: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    let resp = http
        .post("https://github.com/login/device/code")
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[("client_id", client_id), ("scope", SCOPES)])
        .send()
        .await
        .context("could not reach github.com")?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        bail!("GitHub refused the sign-in request ({status}): {text}");
    }
    serde_json::from_str(&text).with_context(|| {
        // An error body ({"error": "..."}) means the OAuth App isn't set up for the device flow.
        format!("GitHub did not start the sign-in: {text}")
    })
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

/// Wait until the user approves the code in the browser. Returns the access token.
pub async fn poll_device_flow(http: &reqwest::Client, client_id: &str, dc: &DeviceCode, cancel: &AtomicBool) -> Result<String> {
    let mut interval = dc.interval.max(5);
    let deadline = std::time::Instant::now() + Duration::from_secs(dc.expires_in.max(60));
    loop {
        for _ in 0..interval * 4 {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if std::time::Instant::now() > deadline {
            bail!("the code expired, start again");
        }
        let resp = http
            .post("https://github.com/login/oauth/access_token")
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", dc.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await;
        let Ok(resp) = resp else { continue }; // transient network trouble: keep polling
        let Ok(body) = resp.json::<TokenResponse>().await else { continue };
        if let Some(token) = body.access_token.filter(|t| !t.is_empty()) {
            return Ok(token);
        }
        match body.error.as_deref() {
            Some("authorization_pending") | None => {}
            Some("slow_down") => interval = body.interval.unwrap_or(interval + 5),
            Some("expired_token") => bail!("the code expired, start again"),
            Some("access_denied") => bail!("sign-in was cancelled in the browser"),
            Some(other) => bail!("{}", body.error_description.unwrap_or_else(|| other.to_string())),
        }
    }
}

/// Token of a signed-in GitHub CLI, if `gh` is installed.
pub fn gh_cli_token() -> Option<String> {
    let mut cmd = std::process::Command::new("gh");
    cmd.args(["auth", "token"]).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!token.is_empty() && !token.contains(char::is_whitespace)).then_some(token)
}

/// Light sanity check of a pasted token (GitHub tokens are ASCII without spaces).
pub fn looks_like_token(t: &str) -> bool {
    let t = t.trim();
    t.len() >= 20 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub mod store {
    use super::*;

    #[cfg(any(windows, target_os = "macos"))]
    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new(crate::backend::APP_ID, "github-token").ok()
    }

    fn file(root: &Path) -> std::path::PathBuf {
        root.join("token")
    }

    pub fn load(root: &Path) -> Option<String> {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(t) = entry().and_then(|e| e.get_password().ok()).filter(|t| !t.is_empty()) {
            return Some(t);
        }
        std::fs::read_to_string(file(root)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    pub fn save(root: &Path, token: &str) -> Result<()> {
        #[cfg(any(windows, target_os = "macos"))]
        if entry().is_some_and(|e| e.set_password(token).is_ok()) {
            let _ = std::fs::remove_file(file(root));
            return Ok(());
        }
        use std::io::Write;
        std::fs::create_dir_all(root)?;
        let path = file(root);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            // Private from the moment it exists, not only after a chmod.
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            opts.mode(0o600);
            if path.exists() {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
        }
        opts.open(&path)?.write_all(token.as_bytes())?;
        Ok(())
    }

    pub fn clear(root: &Path) {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(e) = entry() {
            let _ = e.delete_credential();
        }
        let _ = std::fs::remove_file(file(root));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_shapes() {
        assert!(looks_like_token("ghp_0123456789abcdefABCDEF0123456789abcd"));
        assert!(looks_like_token("  github_pat_11ABCDEFG0123456789_abcdefghijklmnop  "));
        assert!(!looks_like_token("short"));
        assert!(!looks_like_token("has spaces in it which is not a token"));
    }
}
