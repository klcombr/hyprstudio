# HyprStudio — Engineering Report

**Status:** P0 MVP complete; builds, tests pass, runs as a native GTK4/libadwaita application.

HyprStudio is a native Rust desktop suite for Hyprland. It reads the configuration you actually run, exposes appearance, animations, monitors, themes, keybinds, input and window rules as visual editors, previews changes with a simulator, and applies them through a snapshot-backed transaction into a separate managed layer. It never rewrites your configuration files.

## Environment used for verification

| Component | Version |
| --- | --- |
| OS | CachyOS (Arch-based) |
| Rust | 1.99.1 stable |
| GTK | 4.24.1 |
| libadwaita | 1.10.0 |
| Hyprland | 0.57.2, Lua configuration (`hyprland.lua`) |
| SQLite | bundled via `rusqlite` |
| Display | Wayland, 1920×1080 @ scale 1.5 |

Crate layout: one `hyprstudio` package with a library (`src/lib.rs`) and a binary (`src/main.rs`). GTK bindings are `gtk4` 0.11 (`v4_16`) and libadwaita 0.9 (`v1_6`, imported as `adw`).

## Verification performed

### Build

```bash
cargo build
```

Result: success (`Finished dev profile`). 51 compiler warnings remain, all unused imports/dead helper code; no errors.

### Tests

```bash
HYPRSTUDIO_SKIP_HYPRLAND_VERIFY=1 cargo test
```

Result: **180 passed, 0 failed**.

| Suite | Tests |
| --- | --- |
| Library unit tests | 134 |
| `tests/apply_tests.rs` | 5 |
| `tests/backup_tests.rs` | 6 |
| `tests/config_tests.rs` | 15 |
| `tests/ipc_tests.rs` | 9 |
| `tests/preset_tests.rs` | 11 |

Coverage includes descriptors and range validation; both config dialects and import-block idempotency; Hyprland JSON fixtures; preset schema rejection and sparse application; backup integrity/restore; the full apply/rollback transaction; snapshot diff and history behavior. Tests use temporary directories and fixtures only; they never touch `~/.config/hypr`.

`HYPRSTUDIO_SKIP_HYPRLAND_VERIFY=1` is set only for the test run because `hyprland --verify-config` is unavailable/expensive in test context. In normal use the rendered tree is validated with that command before writing.

### Runtime

- The application starts, maps its window (`dev.hyprstudio.HyprStudio`), and renders without a crash after earlier fixes to `set_content` usage and preview replay source handling.
- Screenshots were produced from the live widget tree through GTK's renderer against the real compositor connection, so they show real live values (`Hyprland 0.57.2`, `Default · Lua (.lua)`), not fabricated fixtures.
- No write, import or apply was performed against the user's real configuration during verification.

### Visual review and fixes

The screenshot pass surfaced three real presentation defects, now fixed:

1. Libadwaita preference rows were falling back to the system light palette. Explicit dark theme aliases and boxed-list/row/popover backgrounds were added in `src/ui/style.rs`.
2. Overview metric cards truncated long values. Values/subtitles now wrap; a dedicated `metric-value` size is used.
3. Flat buttons inherited unreadable text colour; `button.flat` now explicitly uses the primary text colour.

Control widths were also tightened (scales 130 px, narrower text/colour fields) so rows fit a 1264 px window.

## Implemented scope

| Area | Status |
| --- | --- |
| Persistent sidebar + status (connection, integration, pending, profile) | Done |
| Overview dashboard | Done |
| Appearance, Animations, Monitors, Themes, Keybinds, Input, Window Rules editors | Done |
| Live simulated preview with replay | Done |
| Presets: versioned TOML, strict validation, import/export | Done |
| History: snapshots, restore, unified diff | Done |
| SQLite persistence (`presets`, snapshots, history, monitor profiles, settings) | Done |
| hyprlang and Lua writers, marked import block | Done |
| Backup → write → `hyprctl reload` → option read-back verification → rollback on failure | Done |
| External-change detection via stored hashes | Done |
| Settings and capability diagnostics | Done |

## Safety model as implemented

- Managed files live under `~/.config/hypr/hyprstudio/`; the only user-file change is a marked import block, and only after the review checkbox.
- Every apply snapshots first; failures restore the snapshot and report recovery success.
- Values are validated against descriptors/registries before writing; the tree is checked with `hyprland --verify-config` when available.
- After reload, effective values are read back. Status distinguishes *Applied and verified*, *Applied with warnings*, *Written, not active* and *Failed*.
- Managed-file hashes detect edits made outside HyprStudio.
- Preset imports deny unknown fields and are size-limited; nothing is executed.

## Screenshots

- `screenshots/hyprstudio.png` — Overview at 1264×675.
- `screenshots/appearance.png` — Appearance settings rows scrolled to the `general` group.

Captured via an offscreen GTK render path added temporarily for the screenshot pass and then removed from the shipped source; the app itself uses no screenshot code.

## Known limitations

- **Lua submaps are unsupported.** Hyprland's public `hl.bind` stub exposes no submap field; submap binds are rejected for Lua and written only for hyprlang.
- **Preview is an approximation.** Blur, shadows, client-side decorations and rounded-corner compositing are simulated.
- **Not a pixel-exact monitor preview.** Multi-output layout is represented, not composited.
- **Unmanaged content is out of scope.** Hand-written Lua logic, `exec-once`, env vars, plugins and GTK theming are untouched.
- **Restore restores files only.** Pending UI values are not re-derived from restored Lua.
- **Conflicts are advisory.** The managed layer loads last and wins while imported; user files are not edited to resolve conflicts.
- **No tray, daemon or auto-apply.**

## What was not verified

- A complete manual click-through of all 11 pages and every dialog.
- An apply against a live user configuration (deliberately avoided; apply logic is covered by temp-tree tests).
- Hardware-specific monitor modes beyond the current output.
- Fractional-scaling appearance other than the 1.5 scale used here.

## Running

```bash
cargo build --release
./target/release/hyprstudio
```

See `README.md` for requirements, safety details, dialect behavior and limitations.
