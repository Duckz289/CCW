# Changelog

## Unreleased - Phase 1 to Phase 3

### Safety

- Removed the force-clean command and every frontend override path.
- Added one shared Rust safety policy with canonical containment checks, symlink/junction/reparse-point rejection, protected-name rules, and validation immediately before mutation.
- Added backend cleanup previews and mandatory frontend confirmation.
- Added structured full/partial/skipped/failed/quarantined outcomes and structured filesystem errors.
- Added atomic state writes with a backup and corruption recovery warnings.
- Sanitized report paths by default; full-path diagnostics require a separate warning and confirmation.

### Quarantine and power tools

- Added same-volume atomic quarantine for Caution targets, manifest persistence, listing, conflict-safe restoration, permanent deletion, and expiry cleanup.
- Added validated Explorer/Finder reveal actions.
- Added bounded largest-file/largest-folder analysis and best-effort file-type breakdown without following symlinks.

### Automation

- Added daily, weekly, monthly, and startup schedules with grace windows and duplicate occurrence markers.
- Added free-space automation controls, cooldowns, volume filtering, and per-run byte limits. Automatic cleanup remains Safe-only.
- Added optional Windows launch-at-login using the current-user Run key and `--minimized`; no service or administrator rights are used.

### Engineering

- Split the Rust backend into commands, models, cleanup, quarantine, safety, scanner, scheduler, state, process, and platform modules.
- Added ESLint flat config, Vitest/Testing Library tests, `typecheck`, `test`, and `check` scripts.
- Added Windows GitHub Actions CI for frontend and Rust verification. No release publishing is configured.
