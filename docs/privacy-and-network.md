# Privacy and network

## What Rimmerge contacts, and why

Rimmerge contacts exactly **two hosts**, both GitHub, for exactly two
purposes. Every request is checked at the point it's made: the scheme
must be `https://`, and the host must be the one host that particular
request is allowed to use — an exact, case-insensitive match against a
fixed list compiled into the program, never a setting, never a
subdomain, never an address taken from a downloaded file.

| Host | What | Size |
|---|---|---|
| `api.github.com` | The latest published Rimmerge release (`/repos/Rimmerge-Project/rimmerge/releases/latest`) — to tell you a newer version exists. Draft and pre-release versions are never announced. | tens of KB |
| `raw.githubusercontent.com` | RimSort's community rules database | about 400 KB |
| `raw.githubusercontent.com` | This project's own `rimmerge-rules.json` | a few KB |
| `raw.githubusercontent.com` | RimSort's Steam Workshop database — **only by a manual action** (a Refresh click, `rimmerge db refresh`, the recommended-databases notice's "Turn on and download" button, or the Dashboard's "Get the recommended rules" step); recommended and enabled by default, but never fetched automatically | about 49 MB |

## If you use a proxy

Rimmerge's HTTP client honours the usual proxy environment variables
(`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, in upper or lower case, with
`NO_PROXY` to exempt hosts). If one is set on your machine, Rimmerge's
requests go through that proxy: the proxy is contacted as well as the two
GitHub hosts, it sees the destination host name (and the request, when it
terminates TLS), and it is outside Rimmerge's control. Rimmerge never sets
or reads a proxy itself; the list of hosts above is still the only
destination it asks for.

## What is sent

Each request is a plain HTTPS GET carrying a `User-Agent: rimmerge`
header (no version, no operating system), the standard headers GitHub's
API requires, and — when Rimmerge already has a copy — the copy's
`ETag`, so an unchanged file isn't downloaded again. No account, token,
cookie, Rimmerge version, profile, install path, mod list, or any other
identifier is ever sent. As with any web request, GitHub sees your IP
address. Rimmerge sends no telemetry and no usage data, to GitHub or
anyone else.

## When it happens

- **On by default, at launch.** After the desktop app has loaded a
  profile, it checks for a new Rimmerge version once per launch, every
  launch (a repeat check costs one small request, and most answer "not
  modified"), and refreshes the community rules and `rimmerge-rules.json`
  once per launch too, but not again within 24 hours of each source's
  last attempt, successful or not. Neither runs again while the app stays
  open; both run in the background, and the app never waits for them.
- **Never before you've seen this explained.** On first launch the app
  shows what it would contact and offers to turn it off. Until you
  answer, it makes no automatic request.
- **Manual actions**: every button that can fetch is listed here, and
  each runs only when you click it.
  - *Check now* (Settings) checks for a new Rimmerge version.
  - *Refresh* (the Rules page's Databases card) refreshes **every
    enabled source**, the 49 MB Steam Workshop database included while
    its switch is on.
  - *Refresh now* on a "rule databases need attention" notice refreshes
    **exactly the sources that notice lists**, nothing else. In
    particular it never downloads the Steam Workshop database unless
    that source is one of the listed ones.
  - *Turn on and download* on the "recommended rule databases" notice
    turns the recommended sources on and refreshes them (including the
    49 MB Steam Workshop database) as one click; it appears only after
    you have answered the first-run notice and never while internet
    access is off.
  - *Get the recommended rules* (the Dashboard's Get started strip)
    turns all three recommended sources on (community rules, the Steam
    Workshop database and `rimmerge-rules`, whose daily automatic fetch
    then resumes) if community rules or the Steam Workshop database is
    switched off. It downloads the ones
    that are switched off or not downloaded yet (including the 49 MB
    Steam Workshop database when it is one of them; the button shows the
    size), and imports them. It needs the first-run notice answered
    and internet access on, unless only an import is left. It never
    runs without the click, and *Skip* declines it.
- **The command-line tool never contacts anything on its own.** Only
  `rimmerge db refresh` and `rimmerge check-update` do, when you run
  them.
- **The Steam Workshop database is never downloaded automatically.** It
  is enabled by default (it is part of the recommended setup), but only
  a manual action downloads it, about 49 MB: the Databases card's
  Refresh, `rimmerge db refresh`, the recommended-databases notice's
  "Turn on and download" button, or the Dashboard's "Get the recommended
  rules" step. On a computer that has never saved
  settings, `rimmerge db refresh` with no `--source` therefore downloads
  it too; turn its switch off first (`rimmerge network set
  --steam-workshop off`, which changes that one switch and leaves
  internet access as it is) to avoid that.

## What a download changes

- A **new version** is only ever announced: Rimmerge never downloads or
  installs an update, and opens no web page unless you ask it to.
- A refreshed **community rules** or **Steam Workshop** database only
  updates a local cache. Your load order doesn't change until you
  choose to import it (the Rules page shows when an import is
  available).
- A refreshed **`rimmerge-rules.json`** is used from the next time a
  profile loads: it improves how `verify` and the findings list model
  specific mods' patch operations and precedence. It never changes the
  sorted load order on its own. See
  [concepts/rules-databases.md](concepts/rules-databases.md).

## Turning it off

- **Settings → Internet access → Allow internet access** (or `rimmerge
  network off`) is the master switch. "Internet access" here means only
  the two GitHub hosts listed above (update checks and rule-database
  refreshes), never your local network. Off, every request — automatic,
  *Check now*, and *Refresh* — is skipped **before any address is built
  or connection opened**. This setting applies to the whole app, on this
  computer, whichever RimWorld install you open.
- Separate switches turn off just the update check, just the automatic
  database refresh, or individual databases.
- Answering the first-run notice (any button, or closing it) saves your
  settings file if there is none yet, so a later version changing a
  default never silently changes what you were shown and answered.
- If the file holding these settings is ever damaged or unreadable,
  Rimmerge treats internet access as **off** until you save your
  settings again.

## Links you click

Separate from network fetches, the desktop app can open a link in your
system's own default browser — never inside the app, and never on its
own. This only ever happens when you click something: a mod's workshop
or homepage link in the mod info panel, or one of the fixed links in
Settings' About section and the sidebar's "Support Rimmerge" link (the
project's GitHub repository, its issues page, and a support page).

Every one of these goes through the same backend opener, and the
frontend never builds or sends a URL to it — a mod link names which of
that mod's own links to open (`"workshop"` or `"homepage"`, resolved
from that mod's own scan data or a re-read `About.xml`), and an app link
names one of a fixed, closed set of targets (the repository, its issues
page, or the support page); the backend maps that name to its own
constant `http`/`https` URL and hands it to the OS. The desktop
frontend's own permission set carries no general "open a URL" capability
at all — only the backend calls the opener, so there's no path from
arbitrary frontend code (or a compromised renderer) to opening an
arbitrary link. A mod's own declared homepage that isn't a valid
`http`/`https` URL (a bare string, a `steam:` link, ...) is shown as
text, never made clickable. A new-version notice's own release link is
copyable text, not a clickable one — copying it to your clipboard never
opens anything on its own.

An imported load order's preview adds one more kind: *Open on Steam
Workshop* beside a listed mod you don't have installed. The list you
imported supplies only that mod's Workshop item id. The frontend sends the
id as digits, never a URL. The backend accepts it only as digits that
make a non-zero 64-bit number, builds the fixed
`https://steamcommunity.com/sharedfiles/filedetails/?id=<id>` address
itself, and hands it to the same opener. A link written in a shared text
list is read for its id and nothing else, so a list can't make Rimmerge
open an address of its choosing. *Copy the missing list* only puts text on
your clipboard.

## Starting the game

The desktop app's **Launch RimWorld** button starts the game only when
you click it (see [Launching RimWorld](desktop.md#launching-rimworld)).
The request it sends to the backend names no path or URL; the backend
works out both from the configured install.

- **A Steam copy** is started by handing the fixed link
  `steam://run/294100` to Windows, which passes it to the Steam client
  already installed on your computer. Rimmerge opens no connection and
  contacts no host to do this: it isn't a web request, the two hosts
  above are still the only ones Rimmerge ever contacts, and the
  Internet access switch has nothing to block here.
- **Steam may go online on its own** once it starts the game, as it
  always does (sign-in, updates, cloud saves). That is Steam's
  behaviour, outside Rimmerge and its settings.
- **Any other copy** (GOG, a standalone folder, or a Steam copy whose
  library isn't listed in a default Steam root's `libraryfolders.vdf`)
  is started by running `RimWorldWin64.exe` from the install folder,
  with no arguments and no shell. Rimmerge doesn't wait for the game or
  read its output.

Launching reads the install folder, Steam's own records of it
(`appmanifest_294100.acf` and `libraryfolders.vdf`) and the list of
running programs, and writes nothing.

## Sharing a load order

Exporting and importing a load order (see
[Sharing a load order](desktop.md#sharing-a-load-order) and the CLI's
[`order export`/`order import`](cli.md)) opens no connection. It is not a
web request, the two hosts above are still the only ones Rimmerge ever
contacts, and the Internet access switch has nothing to block here.

- **Export** reads `ModsConfig.xml` and your installed mods' names and
  Workshop ids. *Save as RimWorld mod list* writes one `.rml` file, at
  the path you choose in the save dialog (the app accepts only a `.rml`
  name and refuses to write over `ModsConfig.xml`); `order export --out`
  writes the one file you name. *Copy as text*, and `order export` without
  `--out`, write no file at all. The exported list holds package ids, mod
  names, Workshop ids and the game's major.minor version, nothing else:
  no path, profile, or account.
- **Import** reads only the file you pick, or the text you paste (or pipe
  to `order import -`), at most 4 MiB. The preview writes nothing. In the
  desktop app, *Use this order* rescans your install with the list, and
  still writes nothing until you Apply. `order import` writes
  `ModsConfig.xml` after printing the same preview, with a backup first,
  unless you pass `--dry-run`.
- **Rimmerge never downloads a mod.** A listed mod you don't have is shown
  with its Workshop link when the list has one (see
  [Links you click](#links-you-click)); installing it is up to you.

## Rate limits and being offline

Being offline is normal: a failed check or refresh is recorded (shown
in Settings and on the Databases card), cached data keeps working, and
Rimmerge simply tries again the next day. If GitHub says its request
limit for your connection has been reached, Rimmerge waits until
GitHub's stated reset time (at most a day) before asking again.

## Everything else is local

Every other operation — scanning your install, sorting, building the
ledger, merging, verifying, sharing a load order, starting the game —
reads and writes only your local filesystem, and only when you ask for a
write. Scanning, sorting, the ledger, `verify`, previewing an imported
load order, and launching RimWorld never write anything; launching only
reads the install folder and Steam's records of it (see
[Starting the game](#starting-the-game)). `apply`, `mods
activate`/`mods deactivate` and `order import` write `ModsConfig.xml`,
with a backup taken first. Exporting a load order writes only the `.rml`
file you chose (see [Sharing a load order](#sharing-a-load-order)).
`apply --write-merge-mod`, and exporting a compatibility patch or
patch-maker mod with `--install`, additionally write a generated mod
folder into your RimWorld install's own `Mods/` folder and add its
package id to `ModsConfig.xml` — the only writes this tool ever makes
under the game install itself. None of these writes happen while
RimWorld looks like it's still running unless you force it. Your
RimWorld install and Workshop content folder are otherwise read only;
Rimmerge's own profile data lives under your local app-data directory.
Rimmerge's own app-data folder also holds `app-settings.json` (these
network settings) and `notifications.json` (which notices you've
dismissed and the last update-check result); each profile's own
`<profile>/notifications.json` also remembers whether you skipped the
Dashboard's "Get the recommended rules" step.

This is a tested contract, not only a stated one: the workspace's
default test suite opens no socket at all. The only tests that talk to
GitHub for real are explicitly separated out and never run by default.
