# Fails on an em dash (U+2014) in text a user can see: a Rust string
# literal (normal, byte, C or raw) under `crates/` and `apps/`, or anywhere
# in `apps/desktop/src/locales/en.json`. The UI copy rule is plain words and
# no em dashes; this keeps backend-generated English (sort rationales, patch
# comments, error messages, CLI output) honest too.
#
# What is NOT flagged:
#   - line, block and doc comments (a comment is not shown to a user);
#   - `#[cfg(test)]` items: an inline test module, a test-only fn/impl/struct/
#     const/use, and the out-of-line `#[cfg(test)] mod x;` files they point at
#     (including `#[path = "..."]`), and `#![cfg(test)]` files;
#   - anything under a `tests/` directory.
# A `#[cfg(test)]` on a struct field, enum variant or match arm only removes
# that one member, never the next `{ ... }` block, so production code after it
# is still scanned.
#
# The check tokenises Rust (nested block comments, raw strings with any number
# of `#`, char literals vs lifetimes) so a dash in a comment on the same line
# as a literal is ignored and a dash after a `//` inside a string is caught.
# A `\u{2014}` escape (any zero padding) inside a literal is flagged too, since
# it renders the same.
#
# Not covered on purpose: the Vue/TS UI. Its standalone "empty value"
# placeholder glyph in templates is exempt and no other check looks at the
# frontend; its prose goes through en.json, which is scanned here.
#
# The dash is built from `[char]0x2014` / `\u2014`, so this file contains none.
#
# Usage (from any directory):
#   pwsh ./scripts/check-em-dashes.ps1             # scan the tree
#   pwsh ./scripts/check-em-dashes.ps1 -SelfTest   # prove the scanner on fixtures
#   pwsh ./scripts/check-em-dashes.ps1 -Root <dir> # scan another tree laid out like this repo

param([switch]$SelfTest, [string]$Root)

$ErrorActionPreference = 'Stop'

$emDash = [string][char]0x2014
$repoRoot = if ($Root) { (Resolve-Path $Root).Path } else { Split-Path -Parent $PSScriptRoot }

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

public static class RustLiteralScanner
{
    public sealed class Result
    {
        // (line, text) of every em dash found inside a string literal.
        public List<KeyValuePair<int, string>> Hits = new List<KeyValuePair<int, string>>();
        // Names of `#[cfg(test)] mod name;` declarations, and "path:<p>" for `#[path = "<p>"]` ones.
        public List<string> TestModules = new List<string>();
        // True when the file carries `#![cfg(test)]`.
        public bool IsWholeFileTest;
    }

    private const char EmDash = '\u2014';

    private static readonly HashSet<string> ItemKeywords = new HashSet<string> {
        "mod", "fn", "impl", "struct", "enum", "trait", "macro_rules",
        "const", "static", "type", "use", "union", "extern"
    };
    private static readonly HashSet<string> Qualifiers = new HashSet<string> {
        "pub", "crate", "super", "self", "in", "unsafe", "async", "default"
    };

    private static bool IsIdent(char c) { return char.IsLetterOrDigit(c) || c == '_'; }

    private static string StripWhitespace(string text)
    {
        var sb = new StringBuilder();
        foreach (char ch in text) if (!char.IsWhiteSpace(ch)) sb.Append(ch);
        return sb.ToString();
    }

    // True when the text holds a `\u{...}` escape whose value is U+2014, however it is padded.
    private static bool ContainsEscapedDash(string text)
    {
        for (int k = 0; k + 1 < text.Length; k++)
        {
            if (text[k] != '\\') continue;
            if (text[k + 1] == 'u' && k + 2 < text.Length && text[k + 2] == '{')
            {
                int close = text.IndexOf('}', k + 3);
                if (close > 0)
                {
                    string hex = text.Substring(k + 3, close - k - 3).Replace("_", "");
                    int value;
                    if (int.TryParse(hex, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out value) && value == 0x2014)
                        return true;
                }
            }
            k++; // the escaped character is consumed, so an escaped backslash never starts an escape
        }
        return false;
    }

    public static Result Scan(string s)
    {
        var result = new Result();
        int n = s.Length, i = 0, line = 1;
        int depth = 0;                 // brace depth
        int skipUntilDepth = -1;       // inside a cfg(test) item: leave it when depth returns here
        bool pending = false;          // saw #[cfg(test)]; the next token decides what it applies to
        bool inHeader = false;         // inside a cfg(test) item's header, before its `{` or `;`
        string headerKeyword = null;
        int headerParen = 0;           // (...) and [...] nesting inside the header
        int visParen = 0;              // `pub(crate)` style parens while pending
        int attrDepth = 0;             // [...] nesting inside a non-cfg(test) attribute
        string lastIdent = null;

        while (i < n)
        {
            char c = s[i];
            if (c == '\n') { line++; i++; continue; }

            if (c == '/' && i + 1 < n && s[i + 1] == '/')
            {
                while (i < n && s[i] != '\n') i++;
                continue;
            }
            if (c == '/' && i + 1 < n && s[i + 1] == '*')
            {
                int nest = 1; i += 2;
                while (i < n && nest > 0)
                {
                    if (s[i] == '\n') line++;
                    if (s[i] == '/' && i + 1 < n && s[i + 1] == '*') { nest++; i += 2; }
                    else if (s[i] == '*' && i + 1 < n && s[i + 1] == '/') { nest--; i += 2; }
                    else i++;
                }
                continue;
            }

            if (c == '#' && attrDepth == 0)
            {
                int j = i + 1;
                bool inner = j < n && s[j] == '!';
                if (inner) j++;
                if (j < n && s[j] == '[')
                {
                    int end = s.IndexOf(']', j);
                    if (end > 0)
                    {
                        string body = StripWhitespace(s.Substring(j + 1, end - j - 1));
                        if (body == "cfg(test)")
                        {
                            if (inner) result.IsWholeFileTest = true;
                            // Inside a skipped item a nested cfg(test) changes nothing.
                            else if (skipUntilDepth < 0) { pending = true; inHeader = false; headerKeyword = null; visParen = 0; }
                            for (int k = i; k <= end; k++) if (s[k] == '\n') line++;
                            i = end + 1;
                            continue;
                        }
                        if (pending && !inHeader && body.StartsWith("path=\"") && body.EndsWith("\""))
                            result.TestModules.Add("path:" + body.Substring(6, body.Length - 7));
                    }
                    // Any other attribute: scan its body as ordinary tokens (a literal in it is user-visible).
                    attrDepth = 1;
                    i = j + 1;
                    continue;
                }
                i++;
                continue;
            }

            bool literalStart = c == '"' || c == '\'';
            if (literalStart && pending && !inHeader && attrDepth == 0) { pending = false; visParen = 0; }
            bool inTest = skipUntilDepth >= 0 || inHeader || (pending && attrDepth > 0);

            // Raw string: (b|c)?r#*"..."#*, not part of a longer identifier.
            if ((c == 'r' || c == 'b' || c == 'c') && (i == 0 || !IsIdent(s[i - 1])))
            {
                int j = i;
                if ((s[j] == 'b' || s[j] == 'c') && j + 1 < n && s[j + 1] == 'r') j++;
                if (s[j] == 'r')
                {
                    int k = j + 1, hashes = 0;
                    while (k < n && s[k] == '#') { hashes++; k++; }
                    if (k < n && s[k] == '"')
                    {
                        if (pending && !inHeader && attrDepth == 0) { pending = false; visParen = 0; }
                        i = ReadRaw(s, k + 1, hashes, ref line, result, skipUntilDepth >= 0 || inHeader);
                        continue;
                    }
                }
            }

            if (c == '"')
            {
                i = ReadString(s, i + 1, ref line, result, inTest);
                continue;
            }

            if (c == '\'')
            {
                // Char literal ('x', '\n', '\u{..}') vs lifetime ('a).
                if (i + 1 < n && s[i + 1] == '\\')
                {
                    int j = i + 3; // skip the escaped character so '\'' closes correctly
                    while (j < n && s[j] != '\'' && s[j] != '\n') j++;
                    string lit = s.Substring(i, Math.Min(j + 1, n) - i);
                    if (!inTest && ContainsEscapedDash(lit))
                        result.Hits.Add(new KeyValuePair<int, string>(line, lit));
                    i = Math.Min(j + 1, n);
                }
                else if (i + 2 < n && s[i + 2] == '\'')
                {
                    if (s[i + 1] == EmDash && !inTest)
                        result.Hits.Add(new KeyValuePair<int, string>(line, "'" + EmDash + "'"));
                    i += 3;
                }
                else i++;
                continue;
            }

            if (IsIdent(c))
            {
                int j = i;
                while (j < n && IsIdent(s[j])) j++;
                string ident = s.Substring(i, j - i);
                if (attrDepth == 0 && pending && !inHeader && visParen == 0)
                {
                    if (ItemKeywords.Contains(ident)) { inHeader = true; headerKeyword = ident; headerParen = 0; }
                    else if (!Qualifiers.Contains(ident)) pending = false; // a field, variant, arm or expression
                }
                lastIdent = ident;
                i = j;
                continue;
            }

            // Punctuation.
            if (attrDepth > 0)
            {
                if (c == '[') attrDepth++;
                else if (c == ']') attrDepth--;
                i++;
                continue;
            }
            if (pending && !inHeader)
            {
                if (c == '(') { visParen++; i++; continue; }
                if (c == ')' && visParen > 0) { visParen--; i++; continue; }
                if (!char.IsWhiteSpace(c)) { pending = false; visParen = 0; }
            }
            if (inHeader)
            {
                if (c == '(' || c == '[') headerParen++;
                else if ((c == ')' || c == ']') && headerParen > 0) headerParen--;
                else if (c == '{' && headerParen == 0)
                {
                    skipUntilDepth = depth;
                    depth++;
                    inHeader = false; pending = false;
                    i++;
                    continue;
                }
                else if (c == ';' && headerParen == 0)
                {
                    // `#[cfg(test)] mod name;` points at an out-of-line test file.
                    if (headerKeyword == "mod" && lastIdent != null) result.TestModules.Add(lastIdent);
                    inHeader = false; pending = false;
                    i++;
                    continue;
                }
            }
            if (c == '{') depth++;
            else if (c == '}')
            {
                depth--;
                if (skipUntilDepth >= 0 && depth == skipUntilDepth) skipUntilDepth = -1;
            }
            i++;
        }
        return result;
    }

    // Reads a normal string body starting after the opening quote; returns the index after the closing quote.
    private static int ReadString(string s, int i, ref int line, Result result, bool inTest)
    {
        int n = s.Length, start = line;
        var sb = new StringBuilder();
        while (i < n && s[i] != '"')
        {
            if (s[i] == '\\' && i + 1 < n)
            {
                sb.Append(s[i]).Append(s[i + 1]);
                if (s[i + 1] == '\n') line++;
                i += 2;
                continue;
            }
            if (s[i] == '\n') { sb.Append('\n'); line++; }
            else sb.Append(s[i]);
            i++;
        }
        if (!inTest) Record(sb.ToString(), start, result, true);
        return Math.Min(i + 1, n);
    }

    private static int ReadRaw(string s, int i, int hashes, ref int line, Result result, bool inTest)
    {
        int n = s.Length, start = line;
        string closer = "\"" + new string('#', hashes);
        int end = s.IndexOf(closer, i, StringComparison.Ordinal);
        if (end < 0) end = n;
        string body = s.Substring(i, end - i);
        foreach (char ch in body) if (ch == '\n') line++;
        // A raw string has no escapes: a literal backslash-u text is not a dash.
        if (!inTest) Record(body, start, result, false);
        return Math.Min(end + closer.Length, n);
    }

    private static void Record(string body, int startLine, Result result, bool hasEscapes)
    {
        string[] lines = body.Split('\n');
        for (int k = 0; k < lines.Length; k++)
        {
            if (lines[k].IndexOf(EmDash) >= 0 || (hasEscapes && ContainsEscapedDash(lines[k])))
                result.Hits.Add(new KeyValuePair<int, string>(startLine + k, lines[k].Trim()));
        }
    }
}
'@

# Returns "path:line: text" for every user-visible em dash found in one .rs file's content.
function Find-LiteralDashes([string]$Path, [string]$Text) {
    $scan = [RustLiteralScanner]::Scan($Text)
    if ($scan.IsWholeFileTest) { return @() }
    $scan.Hits | ForEach-Object { "${Path}:$($_.Key): $($_.Value)" }
}

# Scans the tree under $Root and returns the problem lines (empty when clean).
function Get-EmDashProblem([string]$Root) {
    $files = @(Get-ChildItem -Path (Join-Path $Root 'crates'), (Join-Path $Root 'apps') -Recurse -File -Filter '*.rs' |
        Where-Object { $_.FullName -notmatch '[\\/](target|node_modules)[\\/]' })

    $scans = @{}
    $excluded = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($file in $files) {
        $scan = [RustLiteralScanner]::Scan([System.IO.File]::ReadAllText($file.FullName))
        $scans[$file.FullName] = $scan
        if ($scan.IsWholeFileTest) { [void]$excluded.Add($file.FullName) }
        # `#[cfg(test)] mod name;` in foo.rs points at foo/name.rs; in lib.rs,
        # main.rs or mod.rs it points at name.rs beside it.
        $dir = $file.DirectoryName
        $isRootModule = $file.Name -in @('lib.rs', 'main.rs', 'mod.rs')
        $moduleDir = if ($isRootModule) { $dir } else { Join-Path $dir $file.BaseName }
        foreach ($name in $scan.TestModules) {
            if ($name.StartsWith('path:')) {
                # `#[cfg(test)] #[path = "x/y.rs"] mod m;` is relative to the declaring file's directory.
                [void]$excluded.Add([System.IO.Path]::GetFullPath((Join-Path $dir $name.Substring(5))))
                continue
            }
            [void]$excluded.Add([System.IO.Path]::GetFullPath((Join-Path $moduleDir "$name.rs")))
            [void]$excluded.Add([System.IO.Path]::GetFullPath((Join-Path (Join-Path $moduleDir $name) 'mod.rs')))
        }
    }

    $problems = New-Object 'System.Collections.Generic.List[string]'
    foreach ($file in $files) {
        if ($file.FullName -match '[\\/]tests[\\/]') { continue }
        if ($excluded.Contains($file.FullName)) { continue }
        $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
        foreach ($hit in $scans[$file.FullName].Hits) {
            $problems.Add("${relative}:$($hit.Key): $($hit.Value)")
        }
    }

    $catalogue = Join-Path $Root 'apps/desktop/src/locales/en.json'
    $lineNumber = 0
    foreach ($catalogueLine in [System.IO.File]::ReadLines($catalogue)) {
        $lineNumber++
        if ($catalogueLine.Contains($emDash)) {
            $problems.Add("apps/desktop/src/locales/en.json:${lineNumber}: $($catalogueLine.Trim())")
        }
    }
    return $problems
}

if ($SelfTest) {
    $failed = 0
    function Test-Fixture([string]$Name, [string]$Code, [string[]]$ExpectLines) {
        # ExpectLines: the line numbers a hit must be reported on (empty: no hit).
        $found = @(Find-LiteralDashes 'selftest.rs' $Code.Replace('<D>', $emDash) | ForEach-Object { ($_ -split ':')[1] })
        if (($found -join ',') -ne ($ExpectLines -join ',')) {
            Write-Host "FAIL $Name : expected line(s) [$($ExpectLines -join ',')], got [$($found -join ',')]"
            $script:failed++
        } else {
            Write-Host "ok   $Name ($($found.Count) hit(s))"
        }
    }

    Test-Fixture 'normal literal' 'fn f() -> &str { "a <D> b" }' @('1')
    Test-Fixture 'raw literal with hashes' 'const X: &str = r##"a "# <D> b"##;' @('1')
    Test-Fixture 'byte string' 'const X: &[u8] = b"a <D> b";' @('1')
    Test-Fixture 'raw byte string' 'const X: &[u8] = br#"a <D> b"#;' @('1')
    Test-Fixture 'C string' 'const X: &CStr = c"a <D> b";' @('1')
    Test-Fixture 'escape in literal' 'const X: &str = "a \u{2014} b";' @('1')
    Test-Fixture 'zero-padded escape' 'const X: &str = "a \u{02014} b";' @('1')
    Test-Fixture 'escaped backslash is not an escape' 'const X: &str = "a \\u{2014} b";' @()
    Test-Fixture 'raw string with backslash-u text' 'const X: &str = r"a \u{2014} b";' @()
    Test-Fixture 'char literal' "fn f() -> char { '<D>' }" @('1')
    Test-Fixture 'char escape literal' 'fn f() -> char { ''\u{2014}'' }' @('1')
    Test-Fixture 'line comment' "// a <D> b`nfn f() {}" @()
    Test-Fixture 'doc comment' "/// a <D> b`nfn f() {}" @()
    Test-Fixture 'nested block comment' 'fn f() {} /* a /* <D> */ <D> */' @()
    Test-Fixture 'comment after a literal' 'let x = "ok"; // <D> trailing' @()
    Test-Fixture 'slashes inside a literal' 'let x = "http://x <D> y";' @('1')
    Test-Fixture 'lifetime is not a char' 'fn f<''a>(x: &''a str) -> &''a str { "<D>" }' @('1')
    Test-Fixture 'attribute literal' "#[error(`"a <D> b`")]`nstruct E;" @('1')
    Test-Fixture 'line number inside a multi-line literal' "fn f() {`n    let x = `"a`n  b`n  <D> c`";`n}" @('4')
    Test-Fixture 'line number after a continuation' "let x = `"a \`n   b <D>`";" @('2')
    Test-Fixture 'cfg(test) module' "#[cfg(test)]`nmod tests { fn g() { let _ = `"<D>`"; } }`nfn h() { let _ = `"<D>`"; }" @('3')
    Test-Fixture 'whole-file cfg(test)' "#![cfg(test)]`nfn g() { let _ = `"<D>`"; }" @()
    Test-Fixture 'cfg(test) fn' "#[cfg(test)]`nfn g() { let _ = `"<D>`"; }`nfn h() { let _ = `"<D>`"; }" @('3')
    Test-Fixture 'cfg(test) const with a literal' "#[cfg(test)]`nconst M: &str = `"<D>`";`nfn h() { let _ = `"<D>`"; }" @('3')
    Test-Fixture 'cfg(test) const with an array type' "#[cfg(test)]`nconst X: [u8; 1] = [0];`nfn h() { let _ = `"<D>`"; }" @('3')
    Test-Fixture 'cfg(test) before another attribute' "#[cfg(test)]`n#[derive(Debug)]`nstruct S { a: u8 }`nfn h() { let _ = `"<D>`"; }" @('4')
    Test-Fixture 'cfg(test) on a struct field' "struct R {`n    a: u8,`n    #[cfg(test)]`n    b: u8,`n}`nimpl R { fn m() -> &str { `"<D>`" } }" @('6')
    Test-Fixture 'cfg(test) on a pub(crate) field' "struct R {`n    #[cfg(test)]`n    pub(crate) b: u8,`n}`nimpl R { fn m() -> &str { `"<D>`" } }" @('5')
    Test-Fixture 'cfg(test) on an enum variant' "enum E {`n    A,`n    #[cfg(test)]`n    B,`n}`nimpl E { fn m() -> &str { `"<D>`" } }" @('6')
    Test-Fixture 'cfg(test) on a match arm' "fn f(x: u8) {`n    match x {`n        #[cfg(test)]`n        1 => {}`n        _ => { let _ = `"<D>`"; }`n    }`n}" @('5')
    Test-Fixture 'nested cfg(test) inside a test module' "#[cfg(test)]`nmod t {`n    #[cfg(test)]`n    fn inner() {}`n    fn after() { let _ = `"<D>`"; }`n}`nfn prod() { let _ = `"<D>`"; }" @('7')
    Test-Fixture 'clean file' 'fn f() -> &str { "a - b" }' @()

    $mod = [RustLiteralScanner]::Scan("#[cfg(test)]`nmod helper_tests;`nmod real;")
    if (($mod.TestModules -join ',') -ne 'helper_tests') { Write-Host "FAIL out-of-line cfg(test) module: got '$($mod.TestModules -join ',')'"; $failed++ }
    else { Write-Host 'ok   out-of-line cfg(test) module' }
    $pathMod = [RustLiteralScanner]::Scan("#[cfg(test)]`n#[path = `"sub/t.rs`"]`nmod t;")
    if (($pathMod.TestModules -join ',') -ne 'path:sub/t.rs,t') { Write-Host "FAIL cfg(test) path module: got '$($pathMod.TestModules -join ',')'"; $failed++ }
    else { Write-Host 'ok   cfg(test) #[path] module' }

    # End to end: a throwaway tree with every exclusion and every kind of hit.
    $fixtureRoot = (New-TemporaryFile).FullName
    Remove-Item $fixtureRoot
    try {
        function Write-Fixture([string]$RelativePath, [string]$Content) {
            $target = Join-Path $fixtureRoot $RelativePath
            New-Item -ItemType Directory -Force -Path (Split-Path $target) | Out-Null
            Set-Content -Path $target -Value $Content.Replace('<D>', $emDash) -Encoding utf8
        }
        Write-Fixture 'crates/demo/src/lib.rs' "#[cfg(test)]`nmod helper_tests;`n#[cfg(test)]`n#[path = `"sub/pathed.rs`"]`nmod pathed;`nmod prod;"
        Write-Fixture 'crates/demo/src/helper_tests.rs' 'fn t() -> &str { "test <D> only" }'
        Write-Fixture 'crates/demo/src/sub/pathed.rs' 'fn t() -> &str { "test <D> only" }'
        Write-Fixture 'crates/demo/src/prod.rs' "fn p() -> &str {`n    `"shipped`n    <D> text`"`n}"
        Write-Fixture 'crates/demo/tests/integration.rs' 'fn t() -> &str { "integration <D> only" }'
        Write-Fixture 'apps/desktop/src/locales/en.json' "{`n  `"a`": `"x <D> y`"`n}"
        $expected = @('apps/desktop/src/locales/en.json:2', 'crates/demo/src/prod.rs:3')
        $actual = @(Get-EmDashProblem $fixtureRoot | ForEach-Object { (($_ -split ':')[0, 1] -join ':').Replace('\', '/') } | Sort-Object)
        $expectedSorted = @($expected | Sort-Object)
        if (($actual -join '|') -ne ($expectedSorted -join '|')) {
            Write-Host "FAIL end-to-end tree: expected [$($expectedSorted -join '|')], got [$($actual -join '|')]"
            $failed++
        } else {
            Write-Host 'ok   end-to-end tree (out-of-line and #[path] test modules and tests/ excluded; en.json and prod.rs flagged)'
        }
    } finally {
        Remove-Item -Recurse -Force $fixtureRoot -ErrorAction SilentlyContinue
    }

    if ($failed -gt 0) { throw "check-em-dashes self-test: $failed case(s) failed" }
    Write-Host 'check-em-dashes: self-test passed'
    exit 0
}

$problems = @(Get-EmDashProblem $repoRoot)
if ($problems.Count -gt 0) {
    $problems
    throw "check-em-dashes: $($problems.Count) em dash(es) in user-facing strings; use a colon, comma, parentheses, a full stop or ' - '"
}
Write-Host 'check-em-dashes: clean (Rust string literals under crates/ and apps/, and en.json)'
exit 0
