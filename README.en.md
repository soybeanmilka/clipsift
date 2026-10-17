# ClipSift

> Strip invisible characters from your clipboard: zero-width spaces, fake spaces,
> curly quotes, full-width letters.

**English** | [中文](README.md)

**Pasted C++ from a web page and the compiler says `stray '\342' in program`?**
**Copied Python from Feishu / Notion / Lark and got `IndentationError`?**
**Excel `VLOOKUP` refuses to match two numbers that look identical?**

Not your fault. The text has invisible characters in it. ClipSift takes them out.

```
copy → clean automatically → paste
         ^^^^^^^^^^^^^^
         ClipSift works here
```

## Why you need this

`U+200B` zero-width space (ZWSP), `U+00A0` no-break space (NBSP), `U+2028` line
separator, `U+FEFF` BOM, `U+201C/201D` curly quotes — these characters **look
exactly like normal text**, but they are not the same bytes.

| Situation | What you see | Actual cause |
| --- | --- | --- |
| Paste C++ from a web page | `error: stray '\342' in program` | a `U+200B` slipped into the line |
| Copy Python from Feishu/Notion | `IndentationError: unexpected indent` | indentation is NBSP, not spaces |
| Copy code from Word | syntax errors everywhere | full-width punctuation + curly quotes |
| Excel/ERP exported CSV | `VLOOKUP` won't match | trailing `U+00A0` in the cell |
| Mysterious CLI errors | `command not found` on identical-looking text | copied `U+00A0` instead of a space |
| Prompt-injection by paste | invisible instructions hidden in source | `U+E0000` Unicode Tag / bidi overrides |

Rich-text clipboards from AI chat windows, coding assistants and document editors
are the worst offenders.

- Search by the exact error message → [Troubleshooting index](docs/troubleshooting.md)
- The Feishu/Lark case in depth → [Why copying code from Feishu breaks](docs/feishu-copy-paste.md)

## Install

Grab the package for your platform from
[Releases](https://github.com/soybeanmilka/clipsift/releases):

| Platform | File | Install |
| --- | --- | --- |
| Windows x64 | `clipsift-windows-x86_64.zip` | Unzip, put `clipsift.exe` on your `PATH` (e.g. `C:\Users\<you>\.cargo\bin`) |
| Linux x64 | `clipsift-linux-x86_64.tar.gz` | `tar -xzf` then `sudo mv clipsift /usr/local/bin/` |
| macOS Apple Silicon | `clipsift-macos-aarch64.tar.gz` | `tar -xzf` then `mv clipsift ~/.local/bin/`; you may need `xattr -d com.apple.quarantine clipsift` on first run |

A single executable. No runtime dependencies.

Build from source (Rust 1.74+):

```bash
cargo install --path crates/clipsift-cli
```

## Usage

### The main one: run in the background, clean every copy

```bash
clipsift watch
```

Leave it running. **Every time you copy**, it checks the clipboard and cleans it
in place if needed. Whatever you paste is already clean. Ctrl+C to stop.

Auto-start on Windows: drop a shortcut into `shell:startup`
(press Win+R and type `shell:startup`).

### Clean a file

```bash
# pipe
cat dirty.txt | clipsift > clean.txt

# or point at a file directly
clipsift -f dirty.txt -o clean.txt
```

### See what's dirty first

```bash
clipsift inspect < dirty.txt
```

```
码点         字符       次数       备注
--------------------------------------------------------
U+200B     ·        3        zero-width space
U+00A0     ·        2        no-break space
U+201C     ·        1        curly quote
U+201D     ·        1        curly quote

共 7 处可疑字符。
```

### Clean and get a report

```bash
clipsift --report < dirty.txt
```

```
清除 7 个隐形字符，减少 4 字符
  U+200B  3 次  zero-width space
  U+00A0  2 次  no-break space
  U+201C  1 次  curly quote
  U+201D  1 次  curly quote
```

`--report` prints only the report, not the cleaned text. Drop the flag if you want both.

### Clean the clipboard once

```bash
clipsift once
```

## Rules

The defaults are tested and cover ~95% of real-world cases.

| Key | Name | Default | What it does |
| --- | --- | --- | --- |
| `invisible` | Remove invisible chars | on | Deletes `U+200B/200C/200D/2060/FEFF`, bidi controls, Unicode Tags |
| `spaces` | Normalize fake spaces | on | `U+00A0`, `U+2000-200A`, `U+3000` → plain space |
| `confusables` | Confusables → ASCII | on | Curly quotes `“”‘’`, dashes `—–`, ellipsis `…` → ASCII |
| `fullwidth` | Full-width → half-width | off | `！`→`!`, `Ａ`→`A` (can mangle CJK typography, off by default) |
| `merge` | Merge hard returns | off | For flattening PDF text; **not idempotent**, see below |
| `collapse-ws` | Collapse runs of spaces | on | Multiple spaces within a line → one (**indentation width preserved**) |
| `trim` | Trim line tails | on | Removes trailing spaces/tabs per line |
| `control` | Strip control chars | on | Removes `\0` etc., keeps `\n\r\t` |

### Disable a rule

```bash
clipsift --off fullwidth --off merge < dirty.txt
clipsift --list-rules        # list every key
```

### Config file

```bash
clipsift init > clipsift.json   # write a default config
# edit it, then feed it back
clipsift -c clipsift.json < dirty.txt
```

```json
{
  "invisible": true,
  "normalize_spaces": true,
  "confusables": true,
  "fullwidth_ascii": false,
  "merge_hard_returns": false,
  "collapse_whitespace": true,
  "trim_trailing": true,
  "strip_control": true,
  "custom": []
}
```

Custom regex rules:

```json
{
  "custom": [
    { "name": "去多余空行", "pattern": "\\n{3,}", "replacement": "\n\n", "enable": true }
  ]
}
```

A misspelled key **errors out** instead of being silently ignored.

## Design notes

### Indentation is data, not whitespace

`collapse-ws` only squeezes runs of spaces **within** a line. It never touches
leading indentation, and it preserves the **width** of that indentation. An early
version trimmed both sides of every line, which flattened Python copied from the
web to column zero — that turned a feature into a bug.

### Why "merge hard returns" is not idempotent

The definition of `merge` is self-contradictory with idempotence: a blank line is
a paragraph break (collapse to one `\n`), a single newline is a soft wrap (becomes
a space). The `\n` it produces itself gets eaten as a soft wrap on the next pass.

```
"a⏎⏎b"  --merge-->  "a⏎b"  --merge-->  "a b"
```

So it's off by default, and `clipsift watch` warns if you enable it. It is a
**one-way transform**, only useful for things like flattening PDF body text.

### Rules that delete go before rules that shape

Deleting a control character can bring two previously non-adjacent characters
together, creating a brand-new "trailing space". If the deleting rule ran last,
those newly exposed spaces would wait until the **next** pass — not idempotent.
That's why `strip_control` runs before `collapse-ws`/`trim`.

### The daemon polls instead of using event hooks

Windows' `AddClipboardFormatListener` needs a window handle plus a message loop.
Spinning up a hidden window just to watch the clipboard isn't worth it. A 200ms
poll is imperceptible to humans and costs about 0.01% CPU.

## Verification

```bash
cargo test
```

- 25 unit tests covering each rule and their combinations
- 12 unit tests covering the config file and `--off` switches
- 7 property tests, including **10,000 random dirty strings checked for idempotence**
  (merge-related plans skipped; all others must satisfy `apply(apply(x)) == apply(x)`)
- `cargo clippy --all-targets -- -D warnings` is clean
- End-to-end verified: two passes over a file yield identical SHA256, and the
  clipboard daemon works automatically

## How it compares

| Tool | Auto-cleans | Regex | Cross-platform | Notes |
| --- | --- | --- | --- | --- |
| **ClipSift** | ✅ resident daemon | ✅ | CLI everywhere, daemon is Windows-only | Single file, no deps, transparent rules |
| Gremlins (VS Code ext.) | ❌ highlights only | ❌ | ✅ | Shows, never modifies |
| Invisible AI Character Detector | ❌ manual command | ❌ | ✅ | Triggered by hand inside VS Code |
| Ditto | ✅ needs regex setup | ✅ | Windows | Veteran clipboard manager, heaviest config |
| PureText | ✅ | ❌ | Windows | Strips rich text only, not zero-width chars |
| Manual `sed` | ❌ | ✅ | ✅ | You have to remember to run it |

## License

MIT
