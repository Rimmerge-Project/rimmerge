# Brand assets

The Rimmerge mark: two lanes merging inside a rim, white on the app's
indigo accent (`#4F46E5`, the `--accent` token in
`apps/desktop/src/style.css`).

- `rimmerge-icon.svg` — the mark on a rounded-square tile. The master
  source for every app icon and the web favicon.
- `rimmerge-mark.svg` — the mark alone, transparent, strokes in
  `currentColor`, for inline and README use.
- `rimmerge-icon-1024.png` — 1024x1024 render of the tile.

## Regenerating the app icons

From `apps/desktop`:

```sh
bun run tauri icon ../../docs/brand/rimmerge-icon.svg
```

This rewrites `apps/desktop/src-tauri/icons/`. It also creates `android/`,
`ios/` and `64x64.png`, which the app does not target or reference;
delete them. Copy `rimmerge-icon.svg` to `apps/desktop/public/favicon.svg`
when the tile changes, and re-render the 1024 PNG with
`bun run tauri icon ../../docs/brand/rimmerge-icon.svg -o <scratch dir> -p 1024`.
