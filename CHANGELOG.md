# Changelog

All notable changes to Pirsa are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
