# Akulav Tracker

**Every GitHub repository you own, at a glance — and which ones need you.** Made by Akulav.

A fast native desktop app (Rust + egui) that signs in to GitHub and shows:

- **Overview** — repositories, total commits, lines of code, stars, your commit-day streak, a
  year of weekly commit activity, a contribution calendar across all repositories, and a language
  breakdown.
- **Repositories** — searchable, filterable (mine / private / public / forks / archived / needs
  attention) and sortable by last commit, name, commits, lines or stars. Each row shows a 6-month
  sparkline and a color-coded "last commit" pill.
- **Maintenance radar** — every repository plotted by time since its last commit (today → years),
  grouped into Active · Recent · Cooling · Stale · Dormant, plus a ranked "needs attention" list.
- **Repository detail** — contribution heatmap, weekly commits, code frequency (lines added and
  removed per week), languages, recent commits, and shortcuts to GitHub.

The last sync is cached on disk, so the app opens instantly and only refetches repositories that
changed (or whose statistics are more than a day old).

## Keyboard

| Keys | Does |
| --- | --- |
| `Ctrl+1` … `Ctrl+4` | Overview, Repositories, Maintenance, Settings |
| `Ctrl+F` or `/` | Search repositories |
| `↑` `↓` `PgUp` `PgDn` `Enter` | Move through the list, open a repository |
| `Esc` `Backspace` `Alt+←` mouse back | Go back |
| `F5` / `Ctrl+R` | Sync (`Shift+F5` refetches everything) |

## Signing in

- **Sign in with GitHub** (device flow): shown when the build has an OAuth App client ID. Click,
  copy the code, approve in the browser. No password touches the app.
- **GitHub CLI**: if `gh` is installed and signed in, one click reuses its token.
- **Personal access token**: paste a classic token with `repo`, `read:user`, `read:org` (the
  "Create a token" link preselects them), or a fine-grained token with read access to Contents and
  Metadata.

The token is kept in Windows Credential Manager / macOS Keychain (on Linux in `<data>/token`,
mode 0600). Signing out deletes it and the cache.

### Enabling "Sign in with GitHub"

1. GitHub → Settings → Developer settings → **OAuth Apps** → New OAuth App. Any homepage and
   callback URL (e.g. the repository URL); tick **Enable Device Flow**.
2. Copy the **Client ID** (it is public, not a secret).
3. In this repository: Settings → Secrets and variables → Actions → **Variables** → add
   `OAUTH_CLIENT_ID`. CI bakes it in as `AKULAV_GITHUB_CLIENT_ID`.

For local builds: `$env:AKULAV_GITHUB_CLIENT_ID="Iv1.…"; cargo run` (or set it at run time).

## Install, update, uninstall

One file per OS both installs and updates — the same workflow as the Akulav Launcher:

| OS | File | |
| --- | --- | --- |
| Windows x64 | `AkulavTracker-setup.exe` | Inno Setup, per-user (no admin). Asks for the folder once; updates reuse it. |
| Linux x64 | `AkulavTracker-linux-x64` | `chmod +x` and run; it offers to install itself (menu entry, desktop shortcut, `akulav-tracker` command). |
| macOS arm64 | `AkulavTracker-macos-arm64` | `chmod +x`, right-click → Open; installs itself as `Akulav Tracker.app`. |

- The app checks GitHub Releases at start and every 3 hours (toggle in Settings). Updates are
  downloaded in the background, checked to be an executable for this OS and verified against the
  release's `SHA256SUMS.txt`, then applied with one click: Windows runs the setup silently in
  `/UPDATE` mode into the same folder; Linux/macOS swap the binary in place. The app restarts and
  says which version it came from.
- The install folder holds the program **and** its data (`settings.json`, `cache.json`,
  `avatars/`). `portable.txt` next to the exe, `--portable` or `AKULAV_PORTABLE` keeps data next to
  the program; `AKULAV_DATA_DIR` overrides the folder.
- Uninstall from Settings (or Windows "Apps", or `akulav-tracker --uninstall` on Linux/macOS). It
  asks before deleting the cache, settings and saved sign-in, and never touches other files.

## Releasing

Bump `version` in `Cargo.toml` and push to `main`. The workflow tests and builds all three OSes,
packages the Windows setup with Inno Setup, writes `SHA256SUMS.txt`, tags the commit and publishes
the release. Installed copies pick it up on their next check.

## Development

```bash
cargo run
```

```bash
cargo run -- --demo
```

`--demo` shows made-up repositories without signing in (handy for UI work and screenshots).
`cargo test` runs the unit tests. Build the Windows installer locally with:

```bash
iscc /DAppVersion=1.0.0 /DSourceExe=%CD%\target\release\AkulavTracker.exe installer\akulav-tracker.iss
```

Licensed under the Akulav Source-Available License (see `LICENSE`).
