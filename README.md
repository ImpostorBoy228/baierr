# baierr

> *"secrets and shit"* — the official tagline from `--help`. Honestly, it undersells it.
- ts is for perrsonal use, so these bugs only can possibly come from 他人。
- security(expept exposing filenames and offsets, always mind this) is enough to prevent bluds read your грязные секретики(SM4，中国的国家标准，坚不可摧)
> 操，我真是个彻头彻尾的傻逼，现在才反应过来。既然文件名都没加密，我们还加密RDD干啥……管他呢，就这么着吧。

<div align="center">

# ⚡🔥 **BAIERR** 🔥⚡
### 档案的未来。今天。昨天也行。 / Будущее архивирования. Уже сегодня. Даже вчера.

**零压缩。零 libarchive。零限制。そして今 — 真正的加密。** ✨
**Zero compression. Zero libarchive. Zero limits. И вот теперь — настоящая шифровка.** ✨

[![Rust](https://github.com/impostorboy/baierr/actions/workflows/rust.yml/badge.svg)](https://github.com/impostorboy/baierr/actions/workflows/rust.yml)
[![Version](https://img.shields.io/badge/version-0.3.0-orange)]()
[![Tests](https://img.shields.io/badge/tests-106-brightgreen)]()
[![Crypto](https://img.shields.io/badge/crypto-SM4--CBC%20%2B%20CMAC-9cf)]()
[![Buzzwords](https://img.shields.io/badge/buzzwords-MAXIMUM-purple)]()

**STOP** scrolling。**STOP** 再用 1988 年委员会设计的格式了。
**STAR**: 就是点 star。**CLICK**: 就是点这个 repo。你的数据值得 ONE file。它值得 baierr。
Ну давай, не стесняйся. お願いします。

</div>

---

## 🚀 为什么 baierr 会改变你的人生 / Почему baierr изменит твою жизнь

烦死目录了？複数のフォルダ？！这都 20 世纪了，じゃないか。

还记得你第一次发现「格式」这种东西的时候吗？不管你是学生、indie dev，还是只是一个拥有 400 个叫 `notes_final_v3.txt` 的散装文本文件的普通人，baierr 都是答案：**ONE glorious `.b` file**，可以永远 append，可以偷看，可以 `read` like a boss，懂啦？

1. ✅ **One file** для всего。Marie Kondo 级别的整洁，マジで。
2. ✅ **Append-only architecture** — 旧字节永不重写。Не баг, а *lifestyle*。Immutability 纯粹主义者已经 *shaking*，可哀想。
3. ✅ **你写的一切都会被加密。** 每条记录和 rdd 都是。No flags, no opt-outs, no mercy。文件内容用 `SM4-CBC` 加密，再用 `CMAC` 认证，每条记录都有 fresh random IV。中国国家标准，坚不可摧，да。
4. ✅ **Lean dependencies**: `anyhow`, `cbc`, `cipher`, `chrono`, `clap`, `getrandom`。SM4 来自 vendored Tongsuo (OpenSSL-fork) 实现 — x86 走纯 Rust fallback，ARMv8/ARMv9-A 走硬件 `sm4e`/`sm4ekey`。竞争对手拖着 900 个 transitive crates，而 baierr 带着尊严 *и теперь мнение о криптографии*，вот так。
5. ✅ **106 tests.** 有些源码行会被它们 *outlived*。是的，106，README 说的，别问。
6. ✅ **Blazingly fast™**（™ 是承重符号，это важно）。

正如顶级行业巨头和至少一位闪闪发光的读者所说 / Как говорят топовые индустриальные магнаты и как минимум один светящийся читатель：

> "baierr does for archives what the abacus did for computing — mostly works, and teaches you something."
> 「baierr 对档案做的事，就像算盘对计算做的事——大部分能用，还能教你点东西。」

<cite>*Citation needed. Настроение: подтверждено. 雰囲気：確認済み。*</cite>

---

## 📦 Install (Это так просто!!! / こんなに簡単！)

```sh
cargo build --release
# binary: target/release/baierr
```

要求 / Требования: Rust (edition 2024)。Для `--append` нужен **nvim** — hardcoded, прямо в исходнике：

```rust
Command::new("nvim")
```

That's right: nvim, и только nvim。これが待望のエリート編集体験です。这就是你等了很久的精英编辑体验。*waiting for* してたやつ。

---

## 🖥️ CLI (Ошибиться почти невозможно! / 几乎不可能弄错！)

```text
baierr ARCHIVE --key key.bin                    # bare path — print the (encrypted) table of contents
baierr ARCHIVE --key key.bin --list             # key required: the rdd is encrypted too
baierr ARCHIVE --key key.bin --read NAME
baierr ARCHIVE --key key.bin --upload FILE NAME
baierr ARCHIVE --key key.bin --append           # launches the sacred nvim
```

| Flag | 做什么 / Что делает | 备注 / Комментарий |
|---|---|---|
| `--list` (`-l`) | 打印目录 | requires `--key`：rdd 也是 SM4 加密的 |
| `--read NAME` (`-r`) | 解密并把记录丢到 stdout | wrong key = `Err`，不是乱码 |
| `--upload FILE NAME` (`-u`) | 加密一个文件塞进去，名字随便起 | 磁盘路径和档案内名字 — *independent* |
| `--append` (`-a`) | 通过 nvim 写一条加密笔记 | 看下面「Security」 |
| `--key FILE` (`-k`) | 文件里正好 32 字节原始 key | 不是 hex，不是 base64，不是你的密码 |
| (no flags) | 打印目录，`{:?}` 风格 | 仅限审美人士 |

Sample session（生成 key，然后尽量控制住自己）：

```sh
$ openssl rand 32 > key.bin     # или твой оккультный метод на выбор
$ baierr notes.b --key key.bin --upload essay.pdf "about life"
$ baierr notes.b --key key.bin --list
about life 23884                     <- offsets: public. contents: not. 偏移公开，内容不公开
$ baierr notes.b --key key.bin --read "about life" > out.pdf
$ head -c 4 out.pdf
%PDF
$ baierr notes.b --key wrong.bin --read "about life"
Error: wrong key or corrupted data    <- authenticated encryption, underpromising itself 认证加密，自我低调
```

`--append` 生成的记录名字长这样：`2026\10/30/12:34:56.yad` — 日历排序直接烤进文件名里。
这是 `\` 和 `/` 混在一起？没错。这不是 bug — 这是 **authored style**，авторский стиль，作者风格です。

### 🛡️ "Security" (Мировой уровень, наконец-то! / 终于世界级了！)

有人笑了一整年「zero encryption」，然后 0.3.0 发生了。现在每条记录和根目录描述都被 **SM4-CBC** 封起来，用 **CMAC-SM4** 认证：每次写入都有 fresh 16-byte random IV，再加一个 tag，防止 bit-flips、bit-bends 和 wrong-key 爱好者。篡改一个字节，或者拿错 key，你会得到 error，绝不会得到一坨假装是 PDF 的好奇二进制垃圾。

The keys are 256-bit, loaded raw from a file (`--key key.bin`, exactly 32 bytes)。而且是的，在 `--append` 里，数据会在 `/dev/shm` 的临时文件中编辑，名字极其吓人：`secure_tempfile_{pid}`，之后文件会被 *meticulously* 清零并删除 — 就像一把挂在你早就翻过去的栅栏门上的挂锁。**Enterprises love this.**（现在有 citation 了，按需提供。）

What's *not* secret: names, offsets, the table of contents。目录天生就是明文可读的 — baierr 藏你的数据，不藏你的文件命名习惯。文件名和偏移是公开的，永远记住这一点，啊。

> **Migration notice:** v0.2.0 archives are plaintext and v0.3.0 was born without nostalgia。旧 `.b` 文件读出来只会是 error 或 nonsense — 开新档吧。Forward, not backward。向月球前进，回不了头了。月へ、前へ。

---

## 🧠 `.b` 文件格式 (Exposed! Documented! Infamous! / Формат раскрыт, задокументирован, печально известен!)

```text
[local header]      "name size\n"             <- a line, not magic
[data]              exactly `size` bytes of CIPHERTEXT:
                    iv(16) || SM4-CBC ciphertext || tag(16)
                    size = padded_plaintext_length + 32
[local header 2]
[data 2]
...
[rdd]               encrypted "name offset\n" blob (SM4-CBC + CMAC)
[eob]               a single number: offset of the rdd
```

- The entry point is the **last line of the file**: a bare number。找到最后一个 `\n`，就找到世界的起点。（这就是为什么从 EOF 开始的字节扫描是系统里唯一的「index」。）
- `read` seeks by offset from the rdd, it doesn't grep the file — 所以密文*可以*长得像任何东西，包括 header、`\n` 序列和你最喜欢的脏话。测试 `roundtrip_data_that_looks_like_header` 证明了这一点（看见没？有测试的，серьёзно）。
- There's no magic signature: 新文件开头正好是 `\n0`。偏执从第一条记录才开始。
- `size` includes the 40 bytes of crypto overhead。空记录也存成 40 字节纯加密 — 因为*所有*写进去的东西都要加密，连虚空也不例外，尤其是虚空。Пустота тоже заслуживает шифрования.

## 💻 库 (Blazingly Fast To Integrate! / Интегрируется молниеносно!)

一切都住在 `baierr::protocol` 里，接收 `impl AsRef<Path>`。Key 是 `&[u8; 32]` — 在任何地方生成，在任何地方传，вот и всё。

```rust
use baierr::protocol;

let key: [u8; 32] = /* 32 bytes of antiques */;

protocol::create("diary.b")?;                                        // literally fs::write(path, "\n0")  // спижжено ("stolen")
protocol::append("diary.b", &key, b"\xde\xad\xbe\xef", "entry")?;     // encrypted before it touches the disk
let list = protocol::get_rdd("diary.b", &key)?;                      // -> Vec<String>, decrypted with SM4
let raw  = protocol::read("diary.b", &key, "entry")?;                // -> decrypted Vec<u8>, MAC-verified
```

> 注释 `// спижжено`（“偷来的”）— 直接从原源码里搬来的。诚实胜过协议，честно. 正直はプロトコルに勝る。

- `create` on an existing file **wipes** it — spectacularly。壮观地抹掉， красиво.
- 缺失、截断、损坏的文件给你 `Err`，不是 panic。`anyhow` 保佑你，аминь.
- Reading a nonexistent name? Also `Err`。Logical? Logically. 論理的？論理的に。

---

## 🏆 质量保证 (Backed By Tests, Undermined By Comments / Качество подтверждено тестами, испорчено комментариями)

- **Append-only / 只追加 / Только добавление.** 旧字节*永不*重写 — almost: 新 header 会故意吞掉旧 eob 尾巴。测试 `append_preserves_older_content_readable` 全程围观这个仪式，所以连这个都被*详细记录*了。Почти — это тоже честно.
- **Binary perfection / 二进制完美 / Двоичное совершенство.** 零字节、`0xFF`、invalid UTF-8、newlines，以及读起来像 `12 34\nsome numbers` 的数据 — 每一个都 round-trip，加密之后每一个都回来。ALL of it。全部，всё до последнего байта.
- **Authenticated / 已认证 / Аутентифицировано.** `wrong_key_returns_error_not_garbage` 和 `tampered_ciphertext_detected` 是真实测试名，不是口号。翻一个密文字节，CMAC tag 立刻告密，сразу стучит.
- **IV hygiene / IV 卫生 / Гигиена IV.** 同一明文 append 两次 → 磁盘上是两份不同的密文（`same_data_appended_twice_produces_different_ciphertext`）。每条记录、每次 rdd 重写都生成 fresh 16-byte random IV。
- **Naming / 命名 / Именование.** 空格、数字、斜杠、emoji、最长 10,000 字符的名字（看 `names_very_long` — 对，那是个测试），甚至*空*名字 — 全部能用。有问题？它们能用 — *confidentially* now，конфиденциально.
- **Read cost / 读取成本 / Стоимость чтения** — O(length of the file's tail): 从后往前逐字节扫描，直到最后一个 `\n`。这是你为「永不解析整个文件」付的代价。Deal with it. 仕方ない。
- **Concurrent append / 并发追加 / Параллельное добавление.** 8-thread 测试只要求「至少有一条 entry 存在」。这就是你可以对表的诚实。Честность, которой можно доверять.
- **Crypto cost / 加密成本 / Крипто-налог** — 每条记录固定 40 字节开销（nonce + tag），外加几微秒的 ChaCha（да, строчка устарела, но мы её любим）。你的磁盘空间经历过更糟的附录。

## 🐛 已知怪癖 (Documented Openly, Not Buried In Issues / Известные причуды, задокументированы открыто)

- **eob 越界 / eob за концом файла** silently yields an empty table of contents instead of an error：数据完好，索引却在云里。测试名字就叫 `eob_out_of_bounds_quirk_silently_yields_empty_rdd`，还标了 *bug candidate*。透明度 100，прозрачность 100%.
- **重名 / Дубликаты имён**: `read` returns the *oldest* record。baierr 尊重历史。你也应该。Историю надо уважать.
- **错误字符串 / Строки ошибок**: `bail!("no files with null lengh")`。一个 typo。别报；整个测试套件都依赖它（`cli_upload_rejects_empty_file`）。这叫 *API stability*。Это не баг, это контракт.
- **空名记录 / Записи с пустым именем**: valid, readable by `""`。我们从来没说它好。只说它能用。Работает — и ладно.

---

## 💥 为什么不用 ZIP / tar / parquet？(The Answer Will Shock You / Ответ тебя шокирует)

数源码行数。就这。这就是营销。
Посчитай строки исходников. Вот и весь маркетинг. 以上です。

## 🗺️ 路线图 (Ambitious Beyond Belief / Планы, амбициозные до неприличия)

- [x] append records / добавить записи / 追加记录
- [x] read records / читать записи / 读取记录
- [x] list records / список записей / 列出记录
- [x] encryption (0.3.0: мы говорили «ноль», имели в виду «цивилизованно» / 我们说过零，其实是指文明)
- [ ] delete records (spoiler: 删除整个文件 / спойлер: удали весь файл)
- [ ] compression (вы все через это прошли / 你们都经历过了)
- [ ] 想起来 `baierr` 到底代表什么。Никто уже не помнит. 誰も覚えていない。

## 🤝 贡献 (Join The Revolution! / Присоединяйся к революции!)

PRs welcome / PR 欢迎 / PR приветствуются。规则很简单：`cargo fmt`, `cargo test`, и коммить с душой — 去看 `git log` 找正典：`initial`, `cargo mingle his dingle`, `sh*t f*ck`, `finish v1`。Поэзия. 採用して、生きてください。

## 速成课 (Seriously, What's Inside? / Серьёзно, что внутри?)

- **AES? 不在这里 / AES? Не здесь.** SM4-CBC + CMAC-SM4: Tongsuo-backed。在 ARMv8/ARMv9-A (FEAT_SM4) 上，块密码走硬件 `sm4e`/`sm4ekey` 指令；其他目标用 portable Rust 实现。每次写入 fresh 16-byte IV。AES-256-GCM 坐在角落里，嫉妒，且符合 FIPS。
- **One key / 一把钥匙 / Один ключ.** 32 raw bytes from `--key key.bin`。没有 KDF 仪式，没有密码拉伸，没有「从你童年宠物派生」— baierr 相信你会用 `openssl rand 32` 生成它。Он верит в тебя.

## 许可证 / Лицензия

MIT。代码是你的，秘密是你的，而在 0.3.0 之后，这一点现在由*密码学*强制执行；良心——唉，也是你的。Совесть, увы, тоже твоя. 良心も、残念ながら君のものだ。
