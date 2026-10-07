# Security policy

## Supported versions

Only the latest published release is supported. Please upgrade before
reporting an issue if you're running an older one.

## Reporting a vulnerability

Please report security issues privately, using
[GitHub's private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability)
on the [`Rimmerge-Project/rimmerge`](https://github.com/Rimmerge-Project/rimmerge)
repository (Security tab -> "Report a vulnerability"). Do not open a
public issue for a security report.

## Scope, stated as a guarantee

- **Network access.** Rimmerge contacts exactly **two** hosts, both
  GitHub: `raw.githubusercontent.com` (the three rule databases) and
  `api.github.com` (the release check only) — a closed, compiled-in
  list, never a setting, never a subdomain. The desktop app's checks
  (a new-version check, and a refresh of the community rules and
  `rimmerge-rules` databases) are **on by default and run at launch**
  (the version check each launch, the rule-database refresh at most once
  a day) — but never before the first-run notice has been
  answered, and never from the command line: the CLI only ever contacts
  either host when you run `rimmerge db refresh` or
  `rimmerge check-update` yourself. The Steam Workshop database is on by
  default but never fetched automatically — only a manual `db
  refresh`/Refresh click (or the recommended-databases notice's "Turn on
  and download" button) downloads it. A single master switch
  (`network.allow_network`, `rimmerge network off`) turns every one of
  these off at once: no URL is built and no socket is opened. If the
  file holding that switch is ever unreadable or corrupt, Rimmerge
  treats network access as **off** until you save settings again —
  fails closed, never open. See `docs/privacy-and-network.md` for the
  full policy. A new outbound host, or any request outside this list, is
  a design change and will be treated as a security-relevant one, not a
  routine fix.
- **Filesystem writes.** Rimmerge writes only what you explicitly ask
  for — never on scan, sort, ledger build, or `verify`, and never on a
  dry run:
  - `ModsConfig.xml`, only through `apply`, `mods activate`, or `mods
    deactivate`, always after taking a timestamped backup first.
  - A generated mod folder under `<your RimWorld install>/Mods`, only
    through `apply --write-merge-mod` or `patch export`/`assign export`
    with `--install` (each also adds the generated mod's package id to
    `ModsConfig.xml`, backed up the same way); a merge-mod write also
    keeps the previous generation's own folder as a one-deep backup.
  - Its own profile directory under your local app-data folder.

  None of these writes happen while RimWorld looks like it's still
  running, unless you pass `--force` (CLI) or confirm past the same
  warning (desktop). Rimmerge never writes anywhere else under your
  RimWorld install, and it never touches a RimSort install or its own
  data.

If you find a case where either guarantee above doesn't hold, that's a
security report — please file it privately as described above rather
than as a public bug.
