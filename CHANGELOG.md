# Changelog

All notable changes to Pirsa are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.0] - 2026-11-26

### Added

- File and folder search. Prefixing the query with `~` or `>` searches the
  files and folders in the Windows profile instead of the Start Menu, ranked by
  name. Results show the trailing path and whether the entry is a file or a
  folder; **Enter** or a click opens it with its default handler. The walk is
  bounded (six folder levels, 20,000 entries, 250 ms), debounced, and the newest
  query always wins.
- **Ctrl+1** … **Ctrl+8** launch the nth visible app row, or open the nth file
  result while in file mode.
- **Tab** opens the Settings window from the launcher.
- The launcher remembers its position per monitor in
  `%LOCALAPPDATA%\Pirsa\window.txt` and restores the last-used monitor. A
  missing monitor falls back to the monitor under the foreground window.
- Start Menu support entries — uninstallers, readmes, help files, release notes,
  licence files, and website shortcuts — are filtered out of the app list by
  whole-word match.
- Tests for file-query parsing and ranking, the bounded-walk helpers, the noise
  filter, per-monitor placement math, file-row rendering, and file-mode state.

### Changed

- `pirsa-core` gained a `files` module and `util::is_noise_name`;
  `pirsa-platform` gained `files` (bounded walk and `open_path`) and
  `window_memory` (placement) modules.
- Showing the launcher no longer re-centers it on every open, so a position the
  user dragged the window to survives hide/show and restarts.

## [0.9.0] - 2026-11-26

### Added

- On-disk icon cache under `%LOCALAPPDATA%\Pirsa\icons`. Only apps whose target
  changed are re-read from the shell, and the cache is pruned automatically
  (entries older than 30 days, or beyond 512 files).
- Continuous integration workflow running `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, the test suite, and a benchmark
  build on Windows.
- Criterion benchmark for the search ranking.
- Tests for queued rescans, batched platform requests, accepted and rejected
  hotkey changes, icon pruning, icon-cache encoding, and the partial-index
  warning.

### Changed

- Changing the global hotkey no longer blocks the UI thread: the round-trip to
  the hotkey thread now runs as a task.
- The launcher view borrows one `ViewState` struct instead of implementing a
  16-method trait, so adding a field no longer touches the trait, its
  implementation, and the test fixture.
- A settings window that fails to open no longer leaves a stale window id
  behind; the id is recorded when the window reports that it opened.
- Pressing **F5** during a scan queues one rescan instead of being ignored.
- Partial-index warnings now show their actual message in the footer, and
  **Esc** closes the Settings window.
- The Settings window scrolls its fields, keeping **Close** reachable on small
  or high-DPI displays.
- Icons are keyed by an app's stable search key rather than its display name.
- Shared dependency versions live in `[workspace.dependencies]`.

### Removed

- The synthetic mouse click used as the last-resort foreground fallback. It
  teleported the cursor and could click the wrong target; the Alt-press fallback
  and the focus retry remain.

### Fixed

- Clicking a result activates the app by its stable search key, so a scan that
  lands between rendering and clicking can no longer launch a different app.
- Icons of apps that disappear from the Start Menu are dropped instead of
  lingering in memory.

## [0.8.0] - 2026-11-26

### Added

- Global hotkey setting. The settings window offers Alt+Space (default),
  Ctrl+Space, Ctrl+Alt+Space, Alt+Shift+Space, Ctrl+Shift+Space, Alt+`,
  Ctrl+`, Alt+Q, Ctrl+Alt+Q, and Ctrl+Alt+P. The binding is re-registered
  immediately; if Windows rejects it, the previous hotkey stays active.
- The footer and tray tooltip show the selected hotkey.

### Fixed

- Deeply nested expressions no longer overflow the calculator's parser stack.
  The recursive-descent parser now caps nesting depth, so a pasted query such
  as thousands of nested parentheses is rejected instead of aborting the
  launcher.

## [0.7.0] - 2026-11-26

### Added

- Settings window, opened with **Ctrl+,**, the footer **Settings** button, or the
  tray menu. It offers separate fonts for UI, app names, and category text, plus
  Light, Dark, Nord, Dracula, Gruvbox, Catppuccin, Tokyo Night, and Solarized
  themes. There is intentionally no "follow system" option.
- Appearance settings persist in `%LOCALAPPDATA%\Pirsa\settings.txt`.
- `Ctrl+,` opens settings from the launcher.
- `iced_tester` dev-dependency with headless widget tests for launcher rows, the
  Settings button, and the settings view.
- App-name and category fonts are stored as separate settings. Legacy
  `settings.txt` files with one `font=` entry apply that font to all three.

### Changed

- Launcher colours are now derived from the active iced theme palette instead of
  hardcoded dark colours. Dark remains Pirsa's original custom palette.
- Text in the launcher and settings views uses the selected font.
- Removed the top-right `ALT + SPACE` keycap and close button from the search
  row. The footer still shows the hotkey status; **Esc** hides the launcher.

### Removed

- Removed the unused icon-variation assets and their generator script. The
  Teal Light app icon itself is unchanged.

## [0.6.0] - 2026-11-26

### Added

- Notification-area tray icon. Left-click opens Pirsa; right-click offers
  **Open** and **Quit**. The icon is re-added automatically when Explorer
  restarts.

### Changed

- The app and tray icon now use the transparent, larger **Teal Light**
  (`#5EEAD4`) window glyph instead of the blue rounded-square icon.
- Split the project into a Cargo workspace: `pirsa-core` holds shared models
  and platform-independent logic, `pirsa-platform` owns all Windows/COM/FFI
  integration, and `pirsa-ui` owns the iced view layer. The root `pirsa` binary
  wires them together.
- The UI now consumes a `ViewModel` trait instead of depending on the
  application state type, keeping the UI crate free of platform code.

## [0.5.0] - 2026-11-25

### Added

- Calculator that evaluates arithmetic written in the search field, including
  parentheses, powers, and common functions such as `sqrt`, `sin`, and `log`.
- Length-unit conversion for `mm`, `cm`, `m`, `km`, `in`, `ft`, `yd`, and `mi`,
  with aliases such as `inch`, `feet`, and `metres`. Typing `1 inch` shows a
  primary conversion plus several equivalents; `1 inch to cm` targets one unit.
- The calculation card is selected first. Enter or a click copies the result
  (unit results copy all shown conversions) and hides the launcher.

### Changed

- Start Menu enumeration and icon extraction now run on dedicated, long-lived
  shell workers that initialize COM as an STA. This keeps blocking shell work
  off executor threads, gives every shell call a consistent apartment, and lets
  scans proceed while an icon batch is still running.
- Search metadata is precomputed when apps are indexed: normalized names,
  character vectors, acronyms, and character lengths are stored in each
  `AppEntry`. Rankings no longer normalize every app on every keystroke or
  allocate temporary name/acronym vectors.
- App lists are sorted by the precomputed search key instead of allocating a
  lowercase string for every comparison.
- Usage ranking now uses one history lookup per matched app instead of two.
- Requested icon size is 64 px so icons stay crisp on higher-DPI displays.
- The search hint now mentions calculation and conversion, and seven app rows
  are shown while the calculation card is visible.

### Fixed

- Scan and icon jobs carry generation tokens. If an older scan or icon batch
  finishes late, it can no longer overwrite newer results.
- F5 is ignored while a scan is already running, preventing duplicate queued
  scans on the shell workers.
- `AppEntry` names and details are read-only with precomputed search text, so
  they cannot drift out of sync with the ranker.
- A failed shell worker now returns a visible indexing warning instead of
  silently leaving the launcher empty.

## [0.4.0] - 2026-11-21

### Added

- Clean-shutdown support: the app now creates a named quit event, and the
  installer uses it to stop a running instance before replacing the binary.

### Changed

- `scripts/install.ps1` now detects a running Pirsa instance, asks it to quit
  cleanly, waits for it to exit, and force-stops it only as a fallback before
  installing the new build. The release build runs first, so a failed build no
  longer leaves the previously installed launcher stopped. Installing without
  `-Startup` now removes a stale startup shortcut, and the script verifies that
  the newly launched instance stays alive.
- Single-instance event handling was generalized so the show and quit events
  share one watcher, and installer/process handling was made more defensive.
- Wake-up signalling between the hotkey thread and the iced subscription uses
  `tokio::sync::Notify` instead of a broadcast channel.
- Start Menu normalization is now shared by indexing and search instead of
  being duplicated, and startup/F5 scan handling uses one code path.
- The F5 rescan runs on the task executor instead of blocking the UI thread.
- Duplicate clear/quit button styles were consolidated into one ghost style.
- Removed the "P" logo from the search row so the field uses the full width.
  The app-list heading is now the drag handle for moving the window.

### Fixed

- Subsequence search no longer panics on app names containing non-ASCII
  characters such as `Über`.
- Keyboard selection is limited to the eight rows that are actually visible.

## [0.3.0] - 2026-11-20


### Changed

- Switched rendering from the CPU rasterizer (`tiny-skia` via `softbuffer`) to
  the GPU (`wgpu`, Direct3D 12/Vulkan). The `web-colors` feature keeps
  gamma-space blending, so the palette and translucent highlights look the same
  as before.

## [0.2.0] - 2026-11-20

### Added

- Real Windows application icons in the result list, read from the shell for
  both Start Menu shortcuts and Store/UWP apps, with a colored initial tile as
  a fallback.
- "Frequently used first" ordering: apps that are launched most often rise to
  the top of the default list and win close search ties. The history persists
  in `%LOCALAPPDATA%\Pirsa\usage.txt`.

### Fixed

- Icon colors no longer appear with red and blue swapped. Windows 32-bit DIBs
  store pixels as BGRA while the renderer expects RGBA; the channels are now
  reordered without changing any values.
- The selected-row highlight is now vertically centered around the app icon and
  label instead of leaving the button padding at the bottom.

## [0.1.0] - 2026-11-20

### Added

- Alt+Space global hotkey and a keyboard-first launcher (arrows, Enter, Esc,
  Ctrl+Q, F5).
- Start Menu and `shell:AppsFolder` indexing, including Store/UWP apps.
- Fuzzy ranking with prefix, substring, word-boundary, acronym, and subsequence
  matches.
- Single-instance handling, focus-aware hiding, and a window icon with product
  and version metadata embedded in the executable.
