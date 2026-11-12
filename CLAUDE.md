# CLAUDE.md

Notes for working on **Akulav Tracker** (repo `github-tracker`, crate `akulav-tracker`), made by Akulav: every GitHub
repository the user owns at a glance, and which ones need attention. Rust + eframe/egui. It only talks to GitHub.
One file per OS that installs and updates itself (same model as `akulav-mc-launcher`).

## Commands

```bash
cargo run
cargo test                                   # CI runs: cargo test --release
cargo clippy --all-targets -- -D warnings
cargo build --release
```

## Layout

- `src/backend/` — `github.rs` (API client), `auth.rs` (token sign-in, stored in the OS keyring via `keyring`),
  `cache.rs`, `stats.rs`, `jobs.rs` (background work), `demo.rs` (offline sample data), `settings.rs`, `net.rs`,
  `update.rs` (GitHub releases + `SHA256SUMS.txt`), `install/`
- `src/ui/` — egui app (`app.rs`), pages (`overview.rs`, `repos.rs`, `detail.rs`, `maintenance.rs`, `login.rs`,
  `settings_page.rs`), `charts.rs`, `derive.rs`, `theme.rs`, `widgets.rs`
- `installer/akulav-tracker.iss` — Inno Setup script. **Never change its AppId.**

## Rules

- The GitHub token lives only in the OS keyring. Never write it to settings, cache, logs or error messages.
- Use the demo data for screenshots and UI checks; never commit real account data or tokens (test strings like the
  one in `auth.rs` tests must stay obviously fake).
- Never block the UI thread on network calls; they go through `jobs.rs`. Respect GitHub rate limits (use the cache).
- Release asset names are read by installed copies' updater: keep them. Version in `Cargo.toml`, changes in
  `CHANGELOG.md`; `Cargo.lock` is committed.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships. It is local only (`graphify-out/` is git-ignored).

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost). If graphify-out/ is missing, rebuild it the same way.
