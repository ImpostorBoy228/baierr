# baierr

> *"secrets and shit"* — the official tagline from `--help`. Honestly, it undersells it.

<div align="center">

# ⚡🔥 **BAIERR** 🔥⚡
### The Future Of Archiving. Today. Yesterday, Even.

**Zero compression. Zero encryption. Zero libarchive. Zero limits.** ✨

[![Rust](https://github.com/impostorboy/baierr/actions/workflows/rust.yml/badge.svg)](https://github.com/impostorboy/baierr/actions/workflows/rust.yml)
[![Version](https://img.shields.io/badge/version-0.2.0-orange)]()
[![Tests](https://img.shields.io/badge/tests-90%2B-brightgreen)]()
[![Buzzwords](https://img.shields.io/badge/buzzwords-MAXIMUM-purple)]()

**STOP** scrolling. **STOP** using formats designed by committees in 1988.
**STAR**: as in repositories. **CLICK**: as in star this repo. Your data deserves ONE file. It deserves baierr.

</div>

---

## 🚀 Why baierr Will Change Your Life

Tired of directories? Multiple folders?! So 20th century.

Remember when you first discovered formats? Whether you're a student, an indie dev, or just a person with 400 loose text files named `notes_final_v3.txt`, baierr is the answer: **ONE glorious `.b` file** you can append to forever, peek into, and `read` like a boss:

1. ✅ **One file** for everything. Marie Kondo levels of tidiness.
2. ✅ **Append-only architecture** — old bytes are never rewritten. Not a bug: a *lifestyle*. Immutability purists are *shaking*.
3. ✅ **Almost plaintext.** Open it in `less`, gaze upon it, feel at one with your data.
4. ✅ **Three dependencies total**: `anyhow`, `chrono`, `clap`. While competitors ship with 900 transitive crates, baierr ships with dignity.
5. ✅ **~90 tests.** Some lines of source code will be *outlived* by them.
6. ✅ **Blazingly fast™** (the ™ is load-bearing).

As top-tier industry tycoons and at least one luminous reader say:

> "baierr does for archives what the abacus did for computing — mostly works, and teaches you something."

<cite>*Citation needed. Vibes: confirmed.*</cite>

---

## 📦 Install (It's This Simple!!!)

```sh
cargo build --release
# binary: target/release/baierr
```

Requirements: Rust (edition 2024). For `--append` you need **nvim** — hardcoded, in the source:

```rust
Command::new("nvim")
```

That's right: nvim, and only nvim. This is the elite editor experience you've been *waiting* for.

---

## 🖥️ CLI (Borderline Hard To Get Wrong!)

```text
baierr ARCHIVE                 # bare path — the power move
baierr ARCHIVE --list
baierr ARCHIVE --read NAME
baierr ARCHIVE --upload FILE NAME
baierr ARCHIVE --append        # launches the sacred nvim
```

| Flag | What it does | Comment |
|---|---|---|
| `--list` (`-l`) | print the table of contents | |
| `--read NAME` (`-r`) | dump record to stdout | raw bytes, no questions asked |
| `--upload FILE NAME` (`-u`) | shove a file in, name it anything | disk path and archive name — *independent* |
| `--append` (`-a`) | write a note via nvim | see "Security" below |
| (no flags) | print the table of contents, `{:?}` style | for aesthetes only |

Sample session (try to contain yourself):

```sh
$ baierr notes.b --upload essay.pdf "about life"
$ baierr notes.b --list
about life 23884
$ baierr notes.b --read "about life" > out.pdf
$ head -c 4 out.pdf
%PDF
```

Records from `--append` get names like `2026\10/30/12:34:56.yad` — the calendar
sort order is baked right into the name. Is that `\` mixed with a `/`?
Correct. It's not a bug — it's **authored style**.

### 🛡️ "Security" (World-Class, Allegedly)

In `--append`, data is edited in a temp file in `/dev/shm` with an extremely
intimidating name, `secure_tempfile_{pid}`, and afterward the file is
*meticulously* zeroed out and deleted — exactly like a padlock on a fence gate
you've long since crawled through. But the procedure scratches the conscience,
and the conscience approves. **Enterprises love this.** (To be confirmed.)

---

## 🧠 The `.b` File Format (Exposed! Documented! Infamous!)

```text
[local header]      "name size\n"     <- a line, not magic
[data]              exactly `size` bytes
[local header 2]
[data 2]
...
[rdd]               "name offset\n" — one pair per record
[eob]               a single number: offset of the rdd
```

- The entry point is the **last line of the file**: a bare number. Find the last
  `\n`, find the start of the world. (That's why the byte-scan from EOF is the
  only "index" in the system.)
- `read` seeks by offset from the rdd, it doesn't grep the file — which is why
  data is *allowed* to look like headers. The test
  `roundtrip_data_that_looks_like_header` proves it (see? there are tests).
- There's no magic signature: a fresh file starts with exactly `\n0`.

## 💻 The Library (Blazingly Fast To Integrate!)

Everything lives in `baierr::protocol` and takes `impl AsRef<Path>`.

```rust
use baierr::protocol;

protocol::create("diary.b")?;                                // literally fs::write(path, "\n0")  // спижжено ("stolen")
protocol::append("diary.b", b"\xde\xad\xbe\xef", "entry")?;  // binary, UTF-8 not required
let list = protocol::get_rdd("diary.b")?;                    // -> Vec<String>, in append order
let raw  = protocol::read("diary.b", "entry")?;              // -> Vec<u8>
```

> The comment `// спижжено` ("stolen") — verbatim from the original source. Honesty beats protocols here.

- `create` on an existing file **wipes** it — spectacularly.
- Missing/truncated/corrupted files give you an `Err`, not a panic. `anyhow` blesses you.
- Reading a nonexistent name? Also `Err`. Logical? Logically.

---

## 🏆 Quality Guarantees (Backed By Tests, Undermined By Comments)

- **Append-only.** Old bytes are *never* rewritten — almost: the new header
  deliberately devours the old eob tail. The test
  `append_preserves_older_content_readable` watches this whole ritual, so even
  that is *thoroughly documented*.
- **Binary perfection.** Zero bytes, `0xFF`, invalid UTF-8, newlines, data that
  reads like `12 34\nsome numbers` — every single one round-trips. ALL of it.
- **Naming.** Spaces, digits, slashes, emoji, names up to 10,000 chars (see
  `names_very_long` — yes, that's a test), even *empty* names — all work.
  Something's off? They work.
- **Read cost** — O(length of the file's tail): a byte-by-byte scan backwards
  until the last `\n`. Your payment for never parsing the whole file. Deal with it.
- **Concurrent append.** The 8-thread test merely requires "at least one entry
  present." That's honesty you can set your watch to.

## 🐛 Known Quirks (Documented Openly, Not Buried In Issues)

- **eob past end-of-file** silently yields an empty table of contents instead
  of an error: data intact, index, alas, in the clouds. The test is literally
  named `eob_out_of_bounds_quirk_silently_yields_empty_rdd` and labeled a
  *bug candidate*. Transparency level 100.
- **Duplicate names**: `read` returns the *oldest* record. baierr respects
  history. You should too.
- **Error strings**: `bail!("no files with null lengh")`. A typo. Don't report
  it; the entire test suite depends on it (`cli_upload_rejects_empty_file`).
  This is what we call *API stability*.
- **Empty-name records**: valid, readable by `""`. We never claimed it
  was good. Merely claimed it worked.

---

## 💥 Why Not ZIP / tar / parquet? (The Answer Will Shock You)

Count the lines of source code. That's it. That's the marketing.

## 🗺️ Roadmap (Ambitious Beyond Belief)

- [x] append records
- [x] read records
- [x] list records
- [ ] delete records (spoiler: delete the whole file)
- [ ] compression (you've all been through this)
- [ ] remember what `baierr` even stands for. Nobody remembers anymore

## 🤝 Contributing (Join The Revolution!)

PRs welcome. Rules are simple: `cargo fmt`, `cargo test`, and commit with
soul — check `git log` for the canon: `initial`, `cargo mingle his dingle`,
`sh*t f*ck`, `finish v1`. Poetry. Adopt the style. Live it.

## License

MIT. The code is yours, the secrets are yours, the conscience — alas, also
yours.
