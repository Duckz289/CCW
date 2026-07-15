# Claude Cache Warden

**[English](README.md) · [Tiếng Việt](README.vi.md)**

Claude Cache Warden is a lightweight, local-first desktop application for reviewing and cleaning Claude Desktop / Cowork cache on Windows and macOS. It is built with Tauri v2, Rust, React, and TypeScript.

The app is designed around a safety-first rule: it never treats the frontend as a trusted authority and never lets a protected Claude path become deletable through a force-clean shortcut.

> Current scope: Claude-only cache management. Provider abstraction, cloud sync, accounts, telemetry, and remote log uploads are intentionally out of scope.

## Download

Windows installers are published on the [Releases page](https://github.com/Duckz289/CCW/releases).

- **NSIS Setup EXE** is the recommended installer for most Windows users.
- **MSI** is available for environments that require Windows Installer packages.

## What it does

- Scans known Claude cache roots and shows size, file count, folder count, and safety level.
- Displays a pixel-art treemap to make large cache areas easy to review.
- Requires a backend-generated cleanup preview before every cleanup.
- Produces structured cleanup results: fully cleaned, partially cleaned, skipped, failed, or quarantined.
- Keeps Caution-level items in a restore-capable quarantine instead of deleting them directly.
- Shows largest files/folders, file-type breakdown, and locked or inaccessible items.
- Supports daily, weekly, monthly, startup, size-threshold, and low-disk-space automation.
- Provides tray controls, optional Windows launch-at-login, and start-minimized behavior.
- Exports JSON reports. After either normal or diagnostic export, a popup shows the created file and can open its containing folder.

## Safety model

Every requested path is independently validated in Rust during preview and validated again immediately before mutation.

| Level | Behavior |
| --- | --- |
| **Safe** | Rebuildable cache. It can be cleaned after preview confirmation. |
| **Caution** | Never deleted directly. It can only move atomically to CCW quarantine after an additional confirmation. |
| **Protected** | Claude state, settings, sessions, project/workspace data, identity/browser state, and similar sensitive locations. It cannot be cleaned or quarantined. |

The backend canonicalizes paths and rejects path traversal, links/reparse points, unknown roots, protected branches, overlapping parent/child cleanup selections, and stale paths. Allowing cleanup while Claude is running only changes the process gate; it never weakens the path policy.

### Quarantine

Quarantine is for Caution items only. CCW uses an atomic same-volume move; if that is not possible, the operation fails safely and does not fall back to a partial copy.

Each entry records its original location, size, file count, creation time, retention period, and restore status. Restore requires Claude to be fully closed and refuses to overwrite or merge with an existing original location.

## Privacy

CCW is local-first:

- No telemetry or analytics
- No account or login system
- No cloud sync
- No remote log upload
- No automatic issue submission

Normal exported reports sanitize local home paths (`%USERPROFILE%` on Windows and `~` on macOS). A full-path diagnostic export requires an explicit warning confirmation.

## Automation

Automation is disabled by default and only uses Safe default-cleanup targets.

- Disk-space rules support a volume, minimum free GB, optional free percentage, target free space, cooldown, and a maximum cleanup size.
- Schedules support daily, weekly, monthly, and once-per-app-launch execution. Missed schedules use a grace window and persist an occurrence marker to avoid duplicates.
- Startup cleanup respects its delay, cooldown, Claude activity, and Safe-only policy.
- On Windows, launch-at-login is a current-user registry entry and starts CCW with `--minimized`; no Windows service is created.

## Supported locations

CCW only inspects known Claude locations. Examples include:

- Windows: `%APPDATA%\\Claude`, `%LOCALAPPDATA%\\Claude`, `%LOCALAPPDATA%\\Claude-3p`, `%LOCALAPPDATA%\\Temp\\claude`, and approved Microsoft Store Claude package branches.
- macOS: `~/Library/Application Support/Claude/` and `~/Library/Caches/Claude/` cache branches.

The exact safety classification is decided by the Rust backend at runtime, not by this documentation or the React UI.

## Development

### Requirements

- Node.js 20+
- Rust stable toolchain
- Tauri prerequisites for your OS
  - Windows: Microsoft C++ Build Tools and WebView2 Runtime
  - macOS: Xcode Command Line Tools

### Install and run

```bash
npm install
npm run tauri:dev
```

### Validate

```bash
npm run check
cd src-tauri
cargo fmt --all -- --check
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
```

### Build installers

```bash
npm run tauri:build
```

Windows outputs:

- `src-tauri/target/release/bundle/nsis/*.exe`
- `src-tauri/target/release/bundle/msi/*.msi`

## Current limitations

- Windows runtime and installers are the currently verified release path; macOS behavior still requires platform-specific validation.
- Quarantine only supports same-volume atomic moves.
- File-type classification is best-effort and is based on file names, extensions, and paths; CCW does not inspect file contents.
- Locked-file reporting identifies likely lock/access failures, but does not claim ownership by a specific process.
- The scheduler only runs while CCW is running; CCW does not install a background service.

## License

See the repository license, if provided.
