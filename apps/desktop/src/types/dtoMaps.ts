/**
 * Several generated DTOs (`FieldSpecMapDto`, `RowValueMapDto`,
 * `TargetShapeMapDto`) export as `{ [key in string]?: V }`, not a plain
 * `Record<string, V>`: `ts-rs`'s own `BTreeMap<K, V>` support is what
 * actually threads a working `import` for `V` into the generated file
 * (see `FieldSpecMapDto`'s own Rust-side doc comment,
 * `apps/desktop/src-tauri/src/dto/assignment.rs`, for the `#[ts(type =
 * "Record<...>")]` limitation this works around) — a mapped-type
 * optional index signature is the shape that support produces. Every key
 * actually present in one of these maps carries a real value in practice
 * (the Rust side never serializes an absent one), so application code
 * reads one through this assertion once at the boundary rather than
 * threading `| undefined` through every access. A plain `Record<string,
 * V>` is always assignable back into one of these DTO fields directly —
 * no corresponding "narrow" helper is needed for the write direction.
 */
export function asRecord<V>(map: Partial<Record<string, V>>): Record<string, V> {
  return map as Record<string, V>;
}
