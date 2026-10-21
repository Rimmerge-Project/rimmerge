# Install

The release workflow builds Windows and Linux artifacts. See the artifact table in
[docs/dependency-versions.md](dependency-versions.md#release-artifacts) for the full
list of files on each platform.

## Getting Rimmerge

**Prebuilt releases.** The tagged-release workflow prepares these assets in a
draft release; use the published assets available on
[github.com/Rimmerge-Project/rimmerge/releases](https://github.com/Rimmerge-Project/rimmerge/releases):

- **Windows**: `Rimmerge-<version>-windows-x64-portable.zip` (the desktop
  app `Rimmerge.exe`, the CLI `cli\rimmerge.exe`, and the licenses), a
  CLI-only zip.
- **Linux**: `Rimmerge_<version>_amd64.deb` (desktop, CLI and licenses),
  `Rimmerge-<version>-linux-x64-portable.tar.gz` (desktop + CLI, extract
  anywhere), and a CLI-only tarball.

All artifacts come with `SHA256SUMS.txt`.

**Windows (portable zip):**
1. Download the portable zip and extract it anywhere you like.
2. Run `Rimmerge.exe`. There is no installer: Rimmerge writes nothing
   to the extracted folder, the registry, or Program Files.

**Linux — `.deb` package (Ubuntu 24.04 x86_64 baseline):**
```sh
sudo apt install ./Rimmerge_<version>_amd64.deb
```
The app appears in your launcher as *Rimmerge*; the CLI (`rimmerge`) is
installed to `/usr/bin`.

**Linux — portable tarball (see the support boundary below):**
```sh
tar -xzf Rimmerge-<version>-linux-x64-portable.tar.gz
cd Rimmerge-<version>-linux-x64-portable
./Rimmerge           # desktop app
./cli/rimmerge       # CLI tool
```

The desktop app needs the Microsoft Edge **WebView2** runtime. It is
usually preinstalled on Windows 10 and 11, but not on every LTSC, N, or
unserviced build. If `Rimmerge.exe` opens no window, install the
Evergreen runtime from
[Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).

On Debian/Ubuntu, a Linux desktop development build needs WebKitGTK 4.1,
GTK and the native build tools:

```sh
sudo apt update
sudo apt install build-essential curl file libayatana-appindicator3-dev \
  libssl-dev libwebkit2gtk-4.1-dev libxdo-dev librsvg2-dev wget
```

 Run `bun run tauri dev` from `apps/desktop` in the session you want to validate;
the native folder/file dialogs use `tauri-plugin-dialog` and require the
`dialog:allow-open` and `dialog:allow-save` capabilities already committed
in `src-tauri/capabilities/default.json`.

To use the CLI from any terminal, add the extracted folder's `cli`
subfolder to your `PATH`. The folder is named after the version, so
rename it to `Rimmerge` first, or update the `PATH` entry when you
upgrade.

None of the binaries are code-signed — this project has no signing
certificate. Windows SmartScreen will warn on first run (choose *More
info*, then *Run anyway*); that warning is expected, not a sign the
download was tampered with. To check a download, compare
`(Get-FileHash .\<zip>).Hash.ToLower()` (PowerShell) with the matching
line in `SHA256SUMS.txt`, or run `sha256sum -c SHA256SUMS.txt` (bash)
in the download folder.

**Where your data lives (Windows).** Not in the extracted folder; two
folders under `%LOCALAPPDATA%`:

- `%LOCALAPPDATA%\rimmerge` — settings, profiles, decisions, and cached
  rule databases (see [`config.json`](#configjson) below).
- `%LOCALAPPDATA%\dev.rimmerge.app` — the WebView2 cache and the
  desktop app's display preferences (language, mod-name display).

**Where your data lives (Linux).** Under XDG directories (defaulting to
`~/.local/share`):

- `~/.local/share/rimmerge` — settings, profiles, decisions, and cached
  rule databases.
- `~/.local/share/dev.rimmerge.app` — the WebKitGTK cache and display
  preferences.

Moving the folder or extracting a newer release over it keeps both.

**Uninstalling.** For the `.deb`, run `sudo apt remove rimmerge`. For a
portable archive, delete the extracted folder. To remove
your data as well, also delete both folders above. Portable packages do not
install themselves elsewhere; the `.deb` installs its files through the
package manager. The app does not run at startup.

**Building from source needs the `rules` submodule checked out.**
Rimmerge embeds a snapshot of its
[`rimmerge-rules`](https://github.com/Rimmerge-Project/rimmerge-rules)
data at compile time, from a git submodule at `rules/`. Clone with
`--recursive`, or fetch it into an existing clone:

```sh
git clone --recursive https://github.com/Rimmerge-Project/rimmerge.git
# or, in an existing clone:
git submodule update --init
```

A build without it fails immediately with
`rules/rimmerge-rules.json not found — run: git submodule update --init`
rather than silently shipping without any rule data.

**Building the CLI from source.** Needs a stable Rust toolchain (see
`rust-toolchain.toml` at the workspace root, which pins the channel and
installs automatically if you use `rustup`):

```sh
cargo build --release -p rimmerge-cli
# binary at target/release/rimmerge[.exe]
```

**Building the desktop app from source.** Needs the same Rust
toolchain, Bun, and the platform's Tauri webview dependencies. On Windows
Tauri requires an MSVC host and WebView2; on Linux use the WebKitGTK packages
listed above and an X11 or Wayland session, plus
[Bun](https://bun.sh).

Use Bun **1.4.0** and the frozen lockfile. Build the CLI first: Linux
Debian bundling reads `target/release/rimmerge` from the workspace root.
The release workflow uses the default target directory.

```sh
cargo build --release -p rimmerge-cli
cd apps/desktop
bun install --frozen-lockfile
bun run tauri build
# executable at target/release/rimmerge-desktop[.exe]
# Windows: no installer. Linux: .deb bundle.
```

For development without packaging, use `bun run tauri build --debug --no-bundle`.

`bun run tauri dev` runs it without a release build, for
day-to-day development — see
[apps/desktop/README.md](../apps/desktop/README.md).

To assemble the same packages a release ships from your own build
(after also running `cargo build --release -p rimmerge-cli`):

Windows:
```sh
pwsh ./scripts/package-portable.ps1 -Version 1.0.0 -OutDir out
```

Linux (after `bun run tauri build`):
```sh
pwsh ./scripts/package-linux.ps1 -Version 1.0.0 -OutDir out
```

Use the actual build version in the command. Linux packaging and release
collection require output files to be physically separate from their inputs,
including through symlinks or junctions. Choose a separate output directory;
broken or inaccessible links are refused before copying or cleanup.
Individual output files must be regular files, rather than links.

## Finding your install

Rimmerge needs three paths: the RimWorld game folder, the Steam Workshop
content folder, and `ModsConfig.xml`. It tries to find all three on its
own; every one of them can also be pinned by hand.

## How the game folder is found

In order, the first that resolves wins:

1. `--game-dir` on the CLI, or the Setup page's field for this session
   on the desktop app.
2. the `RIMMERGE_GAME_DIR` environment variable.
3. `game_dir` in the app's own `config.json` (see below).
4. auto-detection, platform-specific — every candidate is just checked
   for existence and RimWorld's own install shape (`Version.txt` +
   `Data/Core/`); nothing is read from the registry or any other
   Windows-specific API:
   - **Windows**: every Steam library named by
     `steamapps\libraryfolders.vdf` under `%ProgramFiles(x86)%\Steam`,
     then under `%ProgramFiles%\Steam` (each library checked for its own
     `steamapps\common\RimWorld`), then those two Steam roots'
     `steamapps\common\RimWorld` directly, then
     `%ProgramFiles(x86)%\GOG Galaxy\Games\RimWorld` and
     `C:\GOG Games\RimWorld`. The Windows registry
     (`HKCU\Software\Valve\Steam\SteamPath`) is deliberately not read —
     it would cost a new dependency for a case the two `%ProgramFiles*%`
     probes already cover, including a library on a second drive
     (`libraryfolders.vdf` under the default root lists every library,
     wherever it actually lives).
   - **macOS**: `~/Library/Application Support/Steam/steamapps/common/RimWorld`,
     then every other Steam library, then `/Applications/RimWorld.app`.
   - **Linux**: the usual native and Flatpak Steam locations, then every
     other Steam library.
5. a typed error naming every candidate it looked in — never a silent
   fallback to a path that doesn't exist.

A path only counts as "the game folder" once it has both `Version.txt`
and a `Data/Core/` subfolder; the same check backs both detection and
every test that needs a real install.

Workshop and `ModsConfig.xml` also support explicit paths,
`RIMMERGE_WORKSHOP_DIR`/`RIMMERGE_MODS_CONFIG` and `config.json`. Workshop
discovery defaults to the game's Steam library. Linux ModsConfig discovery uses
the selected install's layout and registered Steam library: native Unity config,
Flatpak Steam config for a Flatpak-managed native install, or the Proton prefix
in that install's own library. Mixed layouts, conflicting native/Flatpak Steam
ownership, unverifiable Steam records, missing configs and unknown contexts
require an explicit config path.

Native default discovery follows [Unity's Linux configuration convention](https://docs.unity3d.com/2022.3/Documentation/ScriptReference/Application-persistentDataPath.html) and checks
`$XDG_CONFIG_HOME/unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config`;
when XDG_CONFIG_HOME is unset, empty or relative, it falls back to `$HOME/.config`.
Rimmerge's own profile data uses an absolute XDG_DATA_HOME, otherwise
`$HOME/.local/share`. Explicit overrides retain their existing precedence.

## Linux support boundary

| Target | Initial packaging baseline | Formats |
| --- | --- | --- |
| Linux GNU x86_64 / amd64 | Ubuntu 24.04 LTS | Desktop + CLI portable tarball, CLI tarball, `.deb` |

The `.deb` declares WebKitGTK 4.1 and GTK dependencies. The desktop portable
tarball requires compatible system libraries. The CLI can also be downloaded
separately. Both licenses are in each Linux tarball and the `.deb`. See
[Tauri's Debian guidance](https://v2.tauri.app/distribute/debian/).

Native layouts use `RimWorldLinux` and `RimWorldLinux_Data`. Flatpak Steam
and Proton discovery covers common paths and secondary Steam libraries;
Rimmerge itself is not distributed as a Flatpak, and sandbox access or a
particular Wine prefix is not guaranteed. Windows `.exe` files on Linux are
launched through Steam, never directly. A Windows build running through Wine
is distinct from the native build and has no verified Wine-to-native launch
route. Validate launch and running-game refusal using disposable fixtures,
without launching a real game in automated tests.

## `config.json`

Pinning a path once means you never need `--game-dir`/the environment
variable again. It lives next to your profiles, at
`<app data dir>/config.json` (`%LOCALAPPDATA%\rimmerge\config.json` on
Windows):

```json
{
  "schema": 1,
  "game_dir": "<drive>:\\Steam\\steamapps\\common\\RimWorld",
  "workshop_dir": null,
  "mods_config": null
}
```

A field left `null` falls through to auto-detection. From the CLI:

```
rimmerge config show
rimmerge config set --game-dir "<drive>:\Steam\steamapps\common\RimWorld"
rimmerge config clear
```

The desktop app's Setup page writes the same file when you save a path
there.

`config.json` is not `Settings` (see [settings.md](settings.md)) —
settings live inside a profile and change what the sorter produces;
`config.json` only says where to find the game in the first place, so it
has to exist before a profile can even be opened.

## Read-only, until you ask otherwise

Every scan, sort, ledger build, and verify run reads your install and
`ModsConfig.xml` without writing to either. Writes happen only when you
ask for them:

- `apply` (CLI) or the Apply dialog (desktop), and `mods
  activate|deactivate`, write `ModsConfig.xml`.
- `apply --write-merge-mod` and a patch or assignment export with
  `--install` (or the desktop's install option) also write a generated
  mod folder under your install's `Mods` folder and add it to
  `ModsConfig.xml`.

Each `ModsConfig.xml` write is backed up first. See
[quickstart.md](quickstart.md) and [SECURITY.md](../SECURITY.md).
