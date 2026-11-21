# Pirsa

<img src="assets/icon.png" alt="Pirsa icon" width="72" align="right" />

A fast, keyboard-first launcher for Windows, built with **Rust** and **iced**.

Repository: [github.com/gunungpw/pirsa](https://github.com/gunungpw/pirsa)

![Pirsa](docs/screenshot.png)

Press **Alt+Space** anywhere to open it, type a few letters, press **Enter**.
Pirsa searches the Start Menu (including Store/UWP apps) and gets out of the way.

## Features

- **Alt+Space from anywhere** — the app lives in the background with no taskbar
  button.
- **Start Menu search** — indexes both per-user and all-user Start Menu
  shortcuts, plus the Windows `AppsFolder` namespace used by Store/UWP apps such
  as Settings and Calculator.
- **Fuzzy matching** — prefix, substring, word-boundary, acronym (`vsc` →
  Visual Studio Code) and subsequence matches, ranked sensibly.
- **Frequently used first** — apps you launch most often rise to the top of the
  list and win close search ties. The history is saved to
  `%LOCALAPPDATA%\Pirsa\usage.txt`.
- **Real application icons** — every row shows the app's actual Windows icon,
  read from the shell for both Start Menu shortcuts and Store apps, with a
  colored initial tile only as a fallback.
- **Keyboard-first** — arrow keys to move, Enter to launch, Esc to hide.
- **One instance** — launching `pirsa.exe` again reveals the running launcher
  instead of starting a second copy.
- **Focus-aware** — hiding returns focus to the window you came from; clicking
  elsewhere auto-hides the launcher.
- **Native and small** — one executable, no WebView, no runtime dependencies.
- **GPU-accelerated** — frames are drawn with `wgpu` on Direct3D 12/Vulkan
  instead of a CPU rasterizer, using gamma-space (`web-colors`) blending so the
  palette matches the original design.
- **Fast start** — the window appears immediately while Start Menu indexing
  finishes in the background.
- **Proper Windows icon** — embedded in the executable and shortcuts, with
  product/version metadata shown in Explorer and Task Manager.

## Requirements

- Windows 10 or Windows 11
- A Direct3D 12 or Vulkan capable adapter for the `wgpu` renderer
- Rust 1.88 or newer (edition 2024)

## Build

```powershell
cargo build --release
```

The executable is `target\release\pirsa.exe`.

### Static build

Release builds statically link the MSVC C runtime via
[`.cargo/config.toml`](.cargo/config.toml) (`+crt-static`), with
`panic = "abort"`, LTO, and symbol stripping enabled. The resulting `pirsa.exe`
has no dependency on `VCRUNTIME140.dll` or the UCRT and needs no Visual C++
Redistributable. Only Windows system DLLs are imported.

For development:

```powershell
cargo run -- --show
```

`--show` starts with the window visible. Without it, Pirsa starts hidden and waits
for Alt+Space.

## Install (optional)

The install script builds the release binary, copies it to
`%LOCALAPPDATA%\Programs\Pirsa`, creates a Start Menu shortcut, and can
also start Pirsa automatically at login:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -Startup
```

Remove `-Startup` if you prefer to launch Pirsa manually (this also removes a
previously created startup shortcut). Add `-NoLaunch` to install without
starting it immediately.

Re-running the script upgrades in place. If Pirsa is already running, the
script asks it to quit through its named quit event, waits for the process to
exit, and only falls back to force-stopping it if it does not respond. The build
happens before the running copy is touched, so a failed build leaves the
existing installation running.

The app has no tray icon. To quit, open Pirsa and click **Quit** in the footer
(or press **Ctrl+Q** while it is focused).

## Controls

| Key / action | Result |
| --- | --- |
| `Alt+Space` | Open or hide the launcher |
| `↑` / `↓` | Move the selection |
| `Enter` | Launch the selected app |
| `Esc` | Hide the launcher |
| `Ctrl+Q` | Quit Pirsa |
| `F5` | Rescan the Start Menu |
| Click a row | Launch that app |
| Drag the app-list heading | Move the window |

## How indexing works

At startup Pirsa scans:

1. `%APPDATA%\Microsoft\Windows\Start Menu\Programs`
2. `%ProgramData%\Microsoft\Windows\Start Menu\Programs`
3. The Windows `shell:AppsFolder` namespace via the Shell COM API

Shortcuts are de-duplicated by display name, with Start Menu `.lnk`/`.url` files
taking precedence. Store apps are launched through their AppUserModelID; desktop
apps are launched through their shortcut or executable path.

On a typical development machine this indexes 140+ apps in well under a second.
The full scan is synchronous at boot; **F5** refreshes it.

## Why Alt+Space instead of Win+Space?

Windows reserves **Win+Space** for switching keyboard layouts. `RegisterHotKey`
rejects it with error 1409, and intercepting it with a low-level keyboard hook
fights the shell (Start menu, language switcher, and stuck Win-key state are all
easy to trigger). Pirsa therefore uses the reliable, standard **Alt+Space**
global hotkey.

While Pirsa is running, Alt+Space is reserved for the launcher instead of the
active window's system menu.

## Project layout

```text
src/
  main.rs              iced application bootstrap and window settings
  app.rs               launcher state, messages, keyboard handling
  ui.rs                layout, colors, custom widget styles
  search.rs            fuzzy ranking with unit tests
  usage.rs             persistent launch history for the "frequently used first" order
  util.rs              best-effort logging to %LOCALAPPDATA%\Pirsa
  platform/
    mod.rs             single-instance/hotkey startup and shared flags
    hotkey.rs          Alt+Space via RegisterHotKey
    instance.rs        named mutex + named show event
    apps.rs            Start Menu and AppsFolder scanning, ShellExecute
    icons.rs           real app icon extraction via IShellItemImageFactory
    window.rs          show/hide, centering, foreground handling
assets/
  andrsrxn--app-window.svg  source window glyph
  pirsa-icon.svg            composed colored icon
  icon.png / icon.ico      generated icon assets
  icon-64.rgba              raw pixels used for the runtime window icon
scripts/install.ps1    release build/install/startup helper
scripts/make-icon.ps1  regenerate icon assets from the source SVG
build.rs               embeds the icon and version metadata into the exe
```

Regenerate the icon after editing the source SVG (requires ImageMagick 7):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\make-icon.ps1
```

Logs are written to `%LOCALAPPDATA%\Pirsa\launcher.log`.

## Changelog

Release history is documented in [CHANGELOG.md](CHANGELOG.md).

## Limitations

- Only the first eight results are shown; refine the query to narrow further.
- Single-user installation; no MSI installer or auto-updater.

## License

Distributed under the [MIT License](LICENSE).
