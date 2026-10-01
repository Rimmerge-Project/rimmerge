<!--
Thanks for the contribution. Fill in what applies and check off the
gates you actually ran — see CONTRIBUTING.md for how to run each one
and what a real-install tier failure does and doesn't mean for a PR
like this.
-->

## What this does and why

<!-- One or two sentences. Link an issue if there is one. -->

## Gates

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo nextest run --workspace --all-features`
- [ ] `cargo deny check`
- [ ] `pwsh ./scripts/check-forbidden.ps1`
- [ ] `pwsh ./scripts/check-line-endings.ps1`
- [ ] `pwsh ./scripts/check-doc-links.ps1`
- [ ] `apps/desktop`: `bun run types:check && bun run typecheck && bun run lint && bun run test && bun run e2e`

## Checklist

- [ ] Tests added or changed (say which, and what they cover)
- [ ] No local paths, no author names, no specific-mod names in code, tests, or fixtures — `pwsh ./scripts/check-forbidden.ps1` passes
- [ ] Mod-specific knowledge (a load-order pair, a precedence rule, a patch-operation quirk) went to a rules database, not into this code — see CONTRIBUTING.md's "Proposing a load-order rule"
- [ ] Layering respected: `apps/* → rim-session → rim-merge → rim-resolve → rim-analyzer::domain`; `rim-io` implements ports; no IO in `rim-resolve`/`rim-merge`/`rim-analyzer::extract`
- [ ] Docs updated (`docs/…`) if behavior changed, described in present tense
