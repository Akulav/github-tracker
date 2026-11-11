# Changelog

All notable changes to Akulav Tracker are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Older versions are described on the
[releases page](https://github.com/Akulav/github-tracker/releases).

## [1.1.2] - 2026-11-12

### Changed
- New README in the same style as the other Akulav apps: download table, feature cards, per-OS getting started,
  where your data lives and the command line.

### Fixed
- Everything now stays in the folder you installed to: the window size and position are saved there (`window.ron`)
  instead of `%APPDATA%` / `~/.local/share`, and in-app updates download into the install folder (`update\`) instead
  of the system temp folder.
- A copy that is not installed keeps its data next to the program instead of in `%APPDATA%`.
- The uninstaller also removes the downloaded update and the saved window position.
