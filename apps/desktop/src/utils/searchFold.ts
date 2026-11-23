/**
 * The fold every search box compares with: each character lowercased on its own, with the final
 * sigma `ς` written as `σ`. It mirrors `rim-session`'s `search_fold`. Never `toLowerCase()` on the
 * whole string: its final-sigma rule lowercases a trailing `Σ` to `ς` but a middle one to `σ`, so
 * `ΑΣ` would miss `ΑΣΑ`, and a typed `κοσμος` would miss `ΚΟΣΜΟΣ`.
 */
export function searchFold(text: string): string {
  return Array.from(text, (character) => character.toLowerCase().replace("ς", "σ")).join("");
}
