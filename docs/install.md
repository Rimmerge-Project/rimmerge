# Install

## Getting Rimmerge

**Prebuilt releases.** Rimmerge ships as a portable zip, not an
installer. A tagged release carries
`Rimmerge-<version>-windows-x64-portable.zip` (the desktop app
`Rimmerge.exe`, the command-line tool `cli\rimmerge.exe`, and the licenses),
a CLI-only zip, and a `SHA256SUMS.txt`, as GitHub Release assets on
[github.com/Rimmerge-Project/rimmerge/releases](https://github.com/Rimmerge-Project/rimmerge/releases).
If no release exists yet for your platform, build from source.

1. Download the portable zip and extract it anywhere you like.
2. Run `Rimmerge.exe`. There is no installer: Rimmerge writes nothing
   to the extracted folder, the registry, or Program Files.

The desktop app needs the Microsoft Edge **WebView2** runtime. It is
usually preinstalled on Windows 10 and 11, but not on every LTSC, N, or
unserviced build. If `Rimmerge.exe` opens no window, install the
Evergreen runtime from
[Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).

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

**Where your data lives.** Not in the extracted folder; two folders
under `%LOCALAPPDATA%`:

- `%LOCALAPPDATA%\rimmerge` — settings, profiles, decisions, and cached
  rule databases (see [`config.json`](#configjson) below).
- `%LOCALAPPDATA%\dev.rimmerge.app` — the WebView2 cache and the
  desktop app's display preferences (language, mod-name display).

Moving the folder or extracting a newer release over it keeps both.

**Uninstalling.** Delete the extracted folder. To remove your data as
well, also delete both folders above. The app never copies itself
elsewhere and never runs at startup.

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
# binary at target/release/rimmerge.exe
```

**Building the desktop app from source.** Needs the same Rust
toolchain, an MSVC host (Tauri on Windows requires it), and
[bun](https://bun.sh):

```sh
cd apps/desktop
bun install
bun run tauri build
# executable at target/release/rimmerge-desktop.exe (bundling is off:
# no installer is produced)
```

`bun run tauri dev` runs it without a release build, for
day-to-day development — see
[apps/desktop/README.md](../apps/desktop/README.md).

To assemble the same zips a release ships from your own build
(after also running `cargo build --release -p rimmerge-cli`):

```sh
pwsh ./scripts/package-portable.ps1 -Version 1.0.0 -OutDir out
```

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

The workshop folder and `ModsConfig.xml` follow the identical ladder
(`RIMMERGE_WORKSHOP_DIR`/`RIMMERGE_MODS_CONFIG`, then `config.json`,
then a game-folder-relative default).

**Platform support today**: Rimmerge builds and ships for Windows only.
The candidate lists above cover macOS and Linux paths in case that
changes, but neither platform is tested or supported yet.

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
