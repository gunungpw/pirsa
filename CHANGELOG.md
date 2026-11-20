# Changelog

All notable changes to Pirsa are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
