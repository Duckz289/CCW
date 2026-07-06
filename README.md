# Claude Cache Warden

Claude Cache Warden is a lightweight Tauri desktop utility for inspecting and cleaning Claude Desktop / Cowork cache growth on macOS and Windows.

It is intentionally local-only:

- no telemetry
- no backend service
- no admin/root requirement for the supported cache paths
- guarded deletion limited to known Claude cache roots

## Stack

- Tauri v2
- Rust backend
- React + TypeScript frontend
- Tailwind CSS
- Public GitHub API for known-issue status

## Features

- Recursive cache scan with size, file count, folder count, and safety classification.
- Treemap-style visual breakdown.
- Manual cleanup of selected cache directories.
- Process check for Claude Desktop before cleanup.
- Automatic cleanup with OR logic:
  - scheduled time
  - size threshold in GB
- Growth-rate alerting in GB/hour using local samples.
- System tray icon with show, scan, and quit actions.
- Cleanup history.
- JSON report export for bug reports.
- Known Issues tab fetching public GitHub issue status for:
  - anthropics/claude-code#43390
  - anthropics/claude-code#37617
  - anthropics/claude-code#34602

## Mascot States

The React UI uses the pixel-art frog assets in `action/` as the central status layer for the Overview screen.

- Idle: `NORMAL.png` plus `OPEN_CLOSE_EYES/OPEN.png` and `CLOSE.png` loop at a low frame rate while the app is standing by.
- Alert: `ALERT/UP.png` and `DOWN.png` loop when `growth.active === true`, matching the abnormal growth-rate warning.
- Cleaning: `THROW_TRASH/BIN_NOR.png`, `BIN_NO.png`, `BIN_W_FOLDER.png`, and `BIN_FOLDER_END.png` play once when the user presses `Clean now`. After cleanup finishes, the mascot returns to either Idle or Alert based on the refreshed growth state.

The animation is intentionally frame-by-frame with `image-rendering: pixelated`; do not tween or blur between frames.

## Localization

UI copy is separated in `src/i18n.ts` with independent English (`en`) and Vietnamese (`vi`) dictionaries. The header language switch stores the user's choice in `localStorage` under `ccw-language`.

Backend scan data can still contain raw technical paths or unknown folder names. The frontend localizes known cache-root labels, safety descriptions, growth messages, issue states, and cleanup triggers while leaving unknown values unchanged.

## Scanned Paths

macOS:

```text
~/Library/Application Support/Claude/vm_bundles/
~/Library/Application Support/Claude/vm_bundles/warm/
~/Library/Application Support/Claude/Cache/
~/Library/Application Support/Claude/Code Cache/
~/Library/Application Support/Claude/claude-code-vm/
~/Library/Application Support/Claude/claude-code/
~/Library/Caches/Claude/
```

Windows:

```text
%APPDATA%\Claude\
%LOCALAPPDATA%\Claude\
```

The app detects the OS at runtime and resolves the matching user-profile paths.

## Safety Model

Cleanup is blocked unless the selected path is inside a known Claude cache root.

Default cleanup selects only locations explicitly marked for default cleanup, such as verified renderer cache, code cache, and warm VM bundle cache. Some newly observed cache-like locations can still be labeled `Safe` while staying out of the default cleanup set until debug logs confirm their contents. Top-level Claude folders, config-like folders, and session-like folders are classified as `NotRecommended` and are refused by the backend cleanup command.

If Claude Desktop is running, cleanup is blocked unless the user explicitly enables cleanup while Claude is running.

## Known Limitations

- Windows safe-folder classification still depends on verifying real child folder names under both conventional Claude roots and Microsoft Store package roots. In debug builds, or when `CCW_DEBUG_WINDOWS_ROOTS=1` is set, scans log direct child directory names and include one deeper size report for Store `LocalCache` roots so the classifier can be updated from real data instead of guessed names.
- On this Windows test machine, Claude Desktop processes were observed as exact process name `claude` with executable path ending in `Claude.exe`, while `%APPDATA%\Claude\` and `%LOCALAPPDATA%\Claude\` did not exist. The app now detects the Store package root, but keeps newly observed Store cache locations out of automatic/default cleanup until the logged contents are reviewed.
- Automatic cleanup checks the scheduler flag every minute, but full recursive scans are throttled to about every 10 minutes unless the root directory mtimes change. Manual scans still run immediately.

## Development

Install prerequisites:

- Node.js 20+
- Rust stable toolchain
- Tauri platform prerequisites:
  - macOS: Xcode Command Line Tools
  - Windows: Microsoft C++ Build Tools and WebView2 runtime

Install dependencies:

```bash
npm install
```

Run the web UI only:

```bash
npm run dev
```

Run the Tauri app:

```bash
npm run tauri:dev
```

Build production bundles:

```bash
npm run tauri:build
```

## Validation

Frontend:

```bash
npm run build
```

Rust backend:

```bash
cd src-tauri
cargo fmt
cargo check
```

This workspace currently needs Rust/Cargo installed before the Tauri backend can be compiled.
