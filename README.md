# baierr

> *"secrets and shit"* — the official tagline from `--help`. Honestly, it undersells it.

<div align="center">

# ⚡🔥 **BAIERR** 🔥⚡
### The Future Of Archiving. Today. Yesterday, Even.

**Zero compression. Zero libarchive. Zero limits. And now — actual encryption.** ✨

[![Rust](https://github.com/impostorboy/baierr/actions/workflows/rust.yml/badge.svg)](https://github.com/impostorboy/baierr/actions/workflows/rust.yml)
[![Version](https://img.shields.io/badge/version-0.3.0-orange)]()
[![Tests](https://img.shields.io/badge/tests-106-brightgreen)]()
[![Crypto](https://img.shields.io/badge/crypto-XChaCha20--Poly1305-9cf)]()
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
3. ✅ **Everything you write is encrypted.** Every record. No flags, no opt-outs, no mercy. The file *content* is `XChaCha20-Poly1305` ciphertext with a fresh random nonce per record.
4. ✅ **Five dependencies total**: `anyhow`, `chacha20poly1305`, `chrono`, `clap`, `getrandom`. While competitors ship with 900 transitive crates, baierr ships with dignity *and now opinions about cryptography*.
5. ✅ **106 tests.** Some lines of source code will be *outlived* by them.
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
baierr ARCHIVE                                  # bare path — the power move
baierr ARCHIVE --list                           # no key needed. directories are not secrets
baierr ARCHIVE --key key.bin --read NAME
baierr ARCHIVE --key key.bin --upload FILE NAME
baierr ARCHIVE --key key.bin --append           # launches the sacred nvim
```

| Flag | What it does | Comment |
|---|---|---|
| `--list` (`-l`) | print the table of contents | no key: rdd stays plaintext by design |
| `--read NAME` (`-r`) | decrypt and dump record to stdout | wrong key = `Err`, not garbage |
| `--upload FILE NAME` (`-u`) | encrypt a file in, name it anything | disk path and archive name — *independent* |
| `--append` (`-a`) | write an encrypted note via nvim | see "Security" below |
| `--key FILE` (`-k`) | file with exactly 32 bytes of raw key | not hex, not base64, not your password |
| (no flags) | print the table of contents, `{:?}` style | for aesthetes only |

Sample session (generate the key, try to contain yourself):

```sh
$ openssl rand 32 > key.bin     # or your occult method of choice
$ baierr notes.b --key key.bin --upload essay.pdf "about life"
$ baierr notes.b --list
about life 23884                     <- offsets: public. contents: not.
$ baierr notes.b --key key.bin --read "about life" > out.pdf
$ head -c 4 out.pdf
%PDF
$ baierr notes.b --key wrong.bin --read "about life"
Error: wrong key or corrupted data    <- authenticated encryption, underpromising itself
```

Records from `--append` get names like `2026\10/30/12:34:56.yad` — the calendar
sort order is baked right into the name. Is that `\` mixed with a `/`?
Correct. It's not a bug — it's **authored style**.

### 🛡️ "Security" (World-Class, At Last!)

Someone cackled "zero encryption" for a year, and then 0.3.0 happened. Every
record is now sealed with **XChaCha20-Poly1305** (RFC 8439's longer-armed
cousin): a fresh 24-byte random nonce per append and a Poly1305 tag that guards
against bit-flips, bit-bends, and wrong-key enthusiasts. A tampered byte or a
foreign key gets you an error, never a curious binary mess pretending to be
your PDF.

The keys are 256-bit, loaded raw from a file (`--key key.bin`, exactly 32
bytes). And yes, in `--append`, data is edited in a temp file in `/dev/shm`
with an extremely intimidating name, `secure_tempfile_{pid}`, and afterward
the file is *meticulously* zeroed out and deleted — exactly like a padlock on
a fence gate you've long since crawled through. **Enterprises love this.**
(Now with citations available upon request.)

What's *not* secret: names, offsets, the table of contents. Directories are
plainly readable by design — baierr hides your data, not your file naming
habits.

> **Migration notice:** v0.2.0 archives are plaintext and v0.3.0 was born
> without nostalgia. Old `.b` files read as errors or nonsense — start a new
> archive. Forward, not backward. To the moon.

---

## 🧠 The `.b` File Format (Exposed! Documented! Infamous!)

```text
[local header]      "name size\n"             <- a line, not magic
[data]              exactly `size` bytes of CIPHERTEXT:
                    nonce(24) || ciphertext || tag(16)
                    size = plaintext_length + 40
[local header 2]
[data 2]
...
[rdd]               "name offset\n" — one pair per record, plaintext on purpose
[eob]               a single number: offset of the rdd
```

- The entry point is the **last line of the file**: a bare number. Find the last
  `\n`, find the start of the world. (That's why the byte-scan from EOF is the
  only "index" in the system.)
- `read` seeks by offset from the rdd, it doesn't grep the file — which is why
  ciphertext is *allowed* to look like anything, including headers, `\n`
  sequences, and your favorite swear words. The test
  `roundtrip_data_that_looks_like_header` proves it (see? there are tests).
- There's no magic signature: a fresh file starts with exactly `\n0`. Paranoia
  begins at the first record.
- `size` includes the 40 bytes of crypto overhead. Empty records are stored as
  40 bytes of pure crypto — because *everything* written gets encrypted, even
  the void, especially the void.

## 💻 The Library (Blazingly Fast To Integrate!)

Everything lives in `baierr::protocol` and takes `impl AsRef<Path>`. The key is
`&[u8; 32]` — generate it anywhere, pass it everywhere.

```rust
use baierr::protocol;

let key: [u8; 32] = /* 32 bytes of antiques */;

protocol::create("diary.b")?;                                        // literally fs::write(path, "\n0")  // спижжено ("stolen")
protocol::append("diary.b", &key, b"\xde\xad\xbe\xef", "entry")?;     // encrypted before it touches the disk
let list = protocol::get_rdd("diary.b")?;                            // -> Vec<String>, in append order, plaintext
let raw  = protocol::read("diary.b", &key, "entry")?;                // -> decrypted Vec<u8>, MAC-verified
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
  reads like `12 34\nsome numbers` — every single one round-trips, and under
  encryption every single one comes back. ALL of it.
- **Authenticated.** `wrong_key_returns_error_not_garbage` and
  `tampered_ciphertext_detected` are actual test names, not slogans. Flip one
  byte of ciphertext and the Poly1305 tag snitches instantly.
- **Nonce hygiene.** Same plaintext appended twice → different ciphertexts on
  disk (`same_data_appended_twice_produces_different_ciphertext`). XChaCha's
  24-byte nonces make birthday collisions a rounding error at any archive size.
- **Naming.** Spaces, digits, slashes, emoji, names up to 10,000 chars (see
  `names_very_long` — yes, that's a test), even *empty* names — all work.
  Something's off? They work — *confidentially* now.
- **Read cost** — O(length of the file's tail): a byte-by-byte scan backwards
  until the last `\n`. Your payment for never parsing the whole file. Deal with it.
- **Concurrent append.** The 8-thread test merely requires "at least one entry
  present." That's honesty you can set your watch to.
- **Crypto cost** — flat 40 bytes per record (nonce + tag) and microseconds of
  ChaCha on top. Your disk space has survived worse appendices.

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
- [x] encryption (0.3.0: we said zero, we meant civil)
- [ ] delete records (spoiler: delete the whole file)
- [ ] compression (you've all been through this)
- [ ] remember what `baierr` even stands for. Nobody remembers anymore

## 🤝 Contributing (Join The Revolution!)

PRs welcome. Rules are simple: `cargo fmt`, `cargo test`, and commit with
soul — check `git log` for the canon: `initial`, `cargo mingle his dingle`,
`sh*t f*ck`, `finish v1`. Poetry. Adopt the style. Live it.

## Crash Course (Seriously, What's Inside?)

- **AES? Not here.** XChaCha20-Poly1305: pure-Rust, constant-time on any CPU
  without praying to the AES-NI gods, and 24-byte nonces that refuse to collide
  even if you append a million small times. AES-256-GCM sits in the corner,
  jealous, FIPS-compliant.
- **One key.** 32 raw bytes from `--key key.bin`. No KDF ceremony, no password
  stretching, no "derive from your childhood pet" — baierr trusts you to
  generate them with `openssl rand 32`.

## License

MIT. The code is yours, the secrets are yours, and with 0.3.0 that's now
*cryptographically* enforced, the conscience — alas, also yours.
