# Synthetic assemblies

Committed, pre-built `.dll` blobs (`bin/*.dll`) the synthetic install
generator copies verbatim
into generated mod folders, so the analyzer's real PE/CLI metadata
reader (`crates/rim-analyzer/src/extract/pe_metadata.rs`) has real
assembly-reference and runtime-patch evidence to
classify — not a zero-`Hard`-edge fallback.

**Built once by hand, never at fixture-generation time.** The generator
has no compiler dependency; only this one-time build does
(`scripts/build-synthetic-assemblies.ps1`, needs
`C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe` — the .NET
Framework reference assembler already on this workspace's machine).
Rebuild only when a `src/*.cs` file changes:

```powershell
.\scripts\build-synthetic-assemblies.ps1
```

**Rebuilding reproduces classification, not bytes.** `csc.exe` stamps a
fresh COFF header timestamp and a fresh MVID (module version id) into
every build, so two rebuilds of byte-identical source differ by a
handful of bytes even with no `.cs` change — this is normal, not a build
problem to chase down. Nothing in this workspace hashes these blobs or
diffs them byte-for-byte; `synthetic_assemblies.rs` (below) asserts the
*classification* each one produces (assembly name, references,
load-time/lazy, runtime-patch targets/kinds), which is stable across rebuilds.
The **committed** `bin/*.dll` files are the reference — rebuild only when
a `src/*.cs` file actually changes, and expect the diff to touch every
blob's bytes even though only one source did.

## Sources (`src/*.cs`) and what each one proves

- **`MiniPatchLib.dll`** — a minimal, locally-declared stand-in for the
  real runtime-method-patching library: `HarmonyLib.HarmonyPatch(Type,
  string)` and the three bare kind-marker attributes
  (`HarmonyPrefix`/`HarmonyPostfix`/`HarmonyTranspiler`), matching
  exactly the bare type names `pe_metadata`'s attribute reader looks for
  (`resolve_attribute_member_ref` matches by `TypeRef.Name` alone, never
  namespace or source assembly) — without redistributing the real
  library's own compiled DLL, which this workspace has no license to
  ship. Referenced by every `SynthPatchNN.dll` below.
- **`Exports.dll`** — "exports type T": `Example.Exports.BaseWidget`, a
  type another assembly can extend or merely reference.
- **`ExtendsHard.dll`** — "extends T from A": `DerivedWidget : BaseWidget`
  — a `TypeDef.Extends` reference to `Exports`, so `pe_metadata` classifies
  the `AssemblyRef` `load_time: true` (Hard).
- **`CallsSoft.dll`** — "calls T from A only in a method body": a plain
  local variable of type `BaseWidget`, never in `Extends`/`InterfaceImpl`
  — `load_time: false` (Soft).
- **A duplicate-name copy** — no separate source: the generator copies
  `Exports.dll`'s own bytes into a *second* mod folder unmodified. Two
  mods shipping byte-identical `Exports.dll` still carry the same
  `Assembly` table name, exactly the shape `DuplicateAssembly` checks
  (and a real, common case — mods bundling the same shared library
  verbatim), so no fifth source was needed to cover it.
- **`SynthPatch01.dll` .. `SynthPatch12.dll`** — six distinct
  `[HarmonyPatch(typeof(TargetX), "DoX")]` targets, two owners per
  target (twelve files): `TargetAlpha`/`TargetBeta` are patched
  `[HarmonyTranspiler]` by both of their own two owners (a
  `TranspilerCollision` on each, plus the `RuntimePatchCollision` every
  colliding target produces); `TargetGamma`/`TargetDelta`/`TargetEpsilon`/
  `TargetZeta` are each patched once `[HarmonyPrefix]` and once
  `[HarmonyPostfix]` (a `RuntimePatchCollision`, deliberately *not* a
  `TranspilerCollision` — Prefix/Postfix compose safely regardless of
  order). Six targets, two `TranspilerCollision`s.
- **`ExportsSolo.dll`/`ExtendsSolo.dll`** — the
  unambiguous counterpart to `Exports`/`ExtendsHard`: `ExportsSolo` is
  shipped by exactly one mod (`example.framework03`) in the generated
  install, so `ExtendsSolo`'s own load-time reference to it is never
  ambiguous (unlike every `Exports`/`ExtendsHard` reference, which always
  resolves to an `AnyOf` constraint because `Exports` is deliberately
  shipped by two owners) — a real, plain `AssemblyRef` edge, and (three
  dependents ship a copy each) enough real Hard dependents to clear
  `FRAMEWORK_HARD_DEPENDENT_THRESHOLD` (3, `framework_score.rs`), so
  `is_framework_candidate` fires too.
- **`versioned/Exports.dll`** (compiled from
  `ExportsVersioned.cs`, not `Exports.cs`) — the same `Assembly` table
  name ("exports") as `bin/Exports.dll`, stamped version 1.1.0.0 against
  the plain copy's 1.0.0.0 (both explicit — see `Exports.cs`'s own doc
  comment for why an *unversioned* build's implicit `0.0.0.0` would not
  have worked). Two owners of the same assembly name at two real,
  distinct versions is exactly what `AssemblyVersionPrecedence`
  needs; shipped by `example.framework02` in place of a second plain
  copy, so `DuplicateAssembly`/`AnyOf`'s own owner counts for "exports"
  are unaffected — only the version differs.

## Verifying the blobs

`crates/rim-analyzer/tests/pe_metadata_ground_truth.rs` reads the real
install for ground truth and is real-install-gated (out of this fixture's
scope — a different resource).
`crates/rim-analyzer/tests/synthetic_assemblies.rs` is this fixture's own
counterpart: always-run, no install needed, reads every blob in `bin/`
directly through `pe_metadata::read` and asserts the load-time/lazy
classification and runtime-patch target/kind decoding described above.
