//! Signing in to GitHub and keeping the token.
//!
//! Three ways in:
//! * **Sign in with GitHub** — the OAuth device flow (no password ever touches this app). Needs the
//!   client ID of a GitHub OAuth App with "Device flow" enabled, baked in at build time through
//!   `AKULAV_GITHUB_CLIENT_ID` (or set in the environment at run time).
//! * **GitHub CLI** — reuse the token `gh auth token` prints, when `gh` is installed and signed in.
//! * **Personal access token** — pasted by the user.
//!
//! The token is kept only in the OS credential store (see [`store`]). It is never logged and never
//! written to settings, the cache or error messages.

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

/// The token lives only in the OS credential store: Windows Credential Manager, the macOS
/// Keychain, or the Secret Service (GNOME Keyring, KWallet) on Linux. When no store is available
/// the sign-in lasts for this session only; it is never written to a file.
pub mod store {
    use super::*;

    fn entry() -> Result<keyring::Entry> {
        keyring::Entry::new(crate::backend::APP_ID, "github-token").context("the system keychain is not available")
    }

    /// Versions up to 1.2.1 could keep the token in a plain `token` file in the data folder (always
    /// on Linux). It is deleted, not read: such a sign-in has to be made again.
    fn remove_plain_file(root: &Path) {
        let _ = std::fs::remove_file(root.join("token"));
    }

    pub fn load(root: &Path) -> Option<String> {
        remove_plain_file(root);
        entry().ok()?.get_password().ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }

    pub fn save(root: &Path, token: &str) -> Result<()> {
        remove_plain_file(root);
        // The keyring error text never holds the secret itself.
        entry()?.set_password(token).map_err(|e| anyhow::anyhow!("the system keychain refused it: {e}"))
    }

    pub fn clear(root: &Path) {
        remove_plain_file(root);
        if let Ok(e) = entry() {
            let _ = e.delete_credential();
        }
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
