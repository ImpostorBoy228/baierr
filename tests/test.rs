//! Интеграционные тесты: публичный API `baierr::protocol` + подпроцессный CLI.
//! Файл-архив формата: [заголовок][данные]...[rdd][eob] — см. protocol.rs.

use baierr::protocol;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn tmp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "baierr_it_{name}_{}_{}_{:?}_{}.it",
        nanos,
        id,
        std::thread::current().id(),
        std::process::id()
    ))
}
fn cleanup(p: &Path) {
    let _ = std::fs::remove_file(p);
}

const BIN: &str = env!("CARGO_BIN_EXE_baierr");

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
    raw_stdout: Vec<u8>,
}

fn run(args: &[&str]) -> Run {
    let out = Command::new(BIN).args(args).output().unwrap();
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        raw_stdout: out.stdout,
    }
}

// ============================================================
// create
// ============================================================

#[test]
fn create_new_file_marker() {
    let p = tmp_path("create_new");
    protocol::create(&p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"\n0");
    cleanup(&p);
}

#[test]
fn create_overwrites_existing_content() {
    let p = tmp_path("create_ow");
    std::fs::write(&p, b"junk junk junk").unwrap();
    protocol::create(&p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"\n0");
    cleanup(&p);
}

#[test]
fn create_accepts_str_and_pathbuf() {
    let p = tmp_path("create_arg");
    protocol::create(p.to_str().unwrap()).unwrap();
    protocol::create(&p.clone()).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"\n0");
    cleanup(&p);
}

// ============================================================
// append + read roundtrips: разные размеры
// ============================================================

#[test]
fn roundtrip_single_byte() {
    let p = tmp_path("rt1b");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"Z", "one").unwrap();
    assert_eq!(protocol::read(&p, "one").unwrap(), b"Z");
    cleanup(&p);
}

#[test]
fn roundtrip_empty_data() {
    let p = tmp_path("rt0b");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"", "zero").unwrap();
    let d = protocol::read(&p, "zero").unwrap();
    assert!(d.is_empty());
    cleanup(&p);
}

#[test]
fn roundtrip_1kb() {
    let p = tmp_path("rt1k");
    protocol::create(&p).unwrap();
    let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    protocol::append(&p, &data, "kb").unwrap();
    assert_eq!(protocol::read(&p, "kb").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_64kb() {
    let p = tmp_path("rt64k");
    protocol::create(&p).unwrap();
    let data: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
    protocol::append(&p, &data, "o64").unwrap();
    assert_eq!(protocol::read(&p, "o64").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_1mb() {
    let p = tmp_path("rt1m");
    protocol::create(&p).unwrap();
    let data: Vec<u8> = (0..1024 * 1024).map(|i| ((i * 7) % 256) as u8).collect();
    protocol::append(&p, &data, "big1m").unwrap();
    assert_eq!(protocol::read(&p, "big1m").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_highly_negative_bytes_and_nulls() {
    let p = tmp_path("rtnull");
    protocol::create(&p).unwrap();
    let data: Vec<u8> = vec![0, 255, 1, 254, 2, 253, 0, 0, 255];
    protocol::append(&p, &data, "nul").unwrap();
    assert_eq!(protocol::read(&p, "nul").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_only_newlines_data() {
    let p = tmp_path("rtnl");
    protocol::create(&p).unwrap();
    let data = b"\n\n\n\n\n";
    protocol::append(&p, data, "nl").unwrap();
    assert_eq!(protocol::read(&p, "nl").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_data_ending_with_newline() {
    let p = tmp_path("rtendnl");
    protocol::create(&p).unwrap();
    let data = b"hello\n";
    protocol::append(&p, data, "end").unwrap();
    assert_eq!(protocol::read(&p, "end").unwrap(), b"hello\n");
    cleanup(&p);
}

#[test]
fn roundtrip_invalid_utf8_data() {
    let p = tmp_path("rtbin");
    protocol::create(&p).unwrap();
    let data: Vec<u8> = vec![0xff, 0xfe, 0x80, 0x81, b'a', b'\n', 0xff];
    protocol::append(&p, &data, "bin").unwrap();
    assert_eq!(protocol::read(&p, "bin").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_data_that_looks_like_header() {
    // Данные, визуально совпадающие с локальным заголовком, не должны ломать read:
    // read идёт по точному оффсету из rdd, а не ищет текст в файле.
    let p = tmp_path("rtfake");
    protocol::create(&p).unwrap();
    let data = b"fake 42\nnot a header";
    protocol::append(&p, data, "f").unwrap();
    assert_eq!(protocol::read(&p, "f").unwrap(), data);
    cleanup(&p);
}

#[test]
fn roundtrip_data_containing_eob_like_text() {
    let p = tmp_path("rteob");
    protocol::create(&p).unwrap();
    let data = b"12 34\nsome numbers";
    protocol::append(&p, data, "e").unwrap();
    assert_eq!(protocol::read(&p, "e").unwrap(), data);
    cleanup(&p);
}

#[test]
fn append_twice_then_read_both() {
    let p = tmp_path("a2");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"AAA", "first").unwrap();
    protocol::append(&p, b"B\nB", "second").unwrap();
    assert_eq!(protocol::read(&p, "first").unwrap(), b"AAA");
    assert_eq!(protocol::read(&p, "second").unwrap(), b"B\nB");
    cleanup(&p);
}

#[test]
fn append_ten_and_read_backwards() {
    let p = tmp_path("a10");
    protocol::create(&p).unwrap();
    for i in 0..10 {
        let d = format!("val-{}", i).into_bytes();
        protocol::append(&p, &d, &format!("n{}", i)).unwrap();
    }
    for i in (0..10).rev() {
        assert_eq!(
            protocol::read(&p, &format!("n{}", i)).unwrap(),
            format!("val-{}", i).into_bytes()
        );
    }
    cleanup(&p);
}

// ============================================================
// append: инварианты структуры файла
// ============================================================

#[test]
fn layout_after_three_appends_all_offsets_valid() {
    let p = tmp_path("layout3");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"aaa", "f1").unwrap();
    protocol::append(&p, b"bbbbb", "f2").unwrap();
    protocol::append(&p, b"c", "f3").unwrap();
    let bytes = std::fs::read(&p).unwrap();
    // последний rdd-регион начинается c числа в последней строке (eob)
    let last: u64 = std::fs::read_to_string(&p)
        .unwrap()
        .lines()
        .last()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(last > 0 && (last as usize) < bytes.len());
    cleanup(&p);
}

#[test]
fn each_append_increases_file_size() {
    let p = tmp_path("grow3");
    protocol::create(&p).unwrap();
    let mut sizes = vec![std::fs::metadata(&p).unwrap().len()];
    for i in 0..5 {
        protocol::append(&p, &vec![b'x'; 7 + i], &format!("g{}", i)).unwrap();
        sizes.push(std::fs::metadata(&p).unwrap().len());
    }
    for w in sizes.windows(2) {
        assert!(w[1] > w[0]);
    }
    cleanup(&p);
}

#[test]
fn append_preserves_older_content_readable() {
    let p = tmp_path("preserve_bytes");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"OLD-DATA-WITH\nNEWLINES", "old").unwrap();
    let before = std::fs::read(&p).unwrap();
    protocol::append(&p, b"new", "new").unwrap();
    let after = std::fs::read(&p).unwrap();
    // старая eob-строка (` 31` и т.п.) сознательно съедается новым заголовком,
    // поэтому байтовый префикс = всё до последнего \n включительно
    let idx = before.iter().rposition(|&b| b == b'\n').unwrap();
    let prefix = &before[..=idx];
    assert!(
        after.windows(prefix.len()).any(|w| w == prefix),
        "prefix of old content rewritten"
    );
    assert_eq!(protocol::read(&p, "old").unwrap(), b"OLD-DATA-WITH\nNEWLINES");
    assert_eq!(protocol::read(&p, "new").unwrap(), b"new");
    cleanup(&p);
}

#[test]
fn file_ends_with_eob_number_no_trailing_newline() {
    let p = tmp_path("tail");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"x", "t").unwrap();
    let bytes = std::fs::read(&p).unwrap();
    assert_ne!(*bytes.last().unwrap(), b'\n');
    cleanup(&p);
}

// ============================================================
// имена
// ============================================================

#[test]
fn names_with_spaces() {
    let p = tmp_path("n_sp");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"D", "my file v2.txt").unwrap();
    assert_eq!(protocol::read(&p, "my file v2.txt").unwrap(), b"D");
    cleanup(&p);
}

#[test]
fn names_numeric_only() {
    let p = tmp_path("n_num");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"D", "12345").unwrap();
    assert_eq!(protocol::read(&p, "12345").unwrap(), b"D");
    let r = protocol::get_rdd(&p).unwrap();
    assert_eq!(r, vec!["12345"]);
    cleanup(&p);
}

#[test]
fn names_with_inner_spaces_and_digits() {
    // name "a 1": rdd-строка "a 1 <offset>", rsplit_once(' ') даёт name = "a 1"
    let p = tmp_path("n_mix");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"D", "a 1").unwrap();
    assert_eq!(protocol::read(&p, "a 1").unwrap(), b"D");
    cleanup(&p);
}

#[test]
fn names_slashes_and_unicode() {
    let p = tmp_path("n_uni");
    protocol::create(&p).unwrap();
    let name = "2026/10/30:фото-😀.png";
    protocol::append(&p, b"U", name).unwrap();
    assert_eq!(protocol::read(&p, name).unwrap(), b"U");
    assert_eq!(protocol::get_rdd(&p).unwrap(), vec![name]);
    cleanup(&p);
}

#[test]
fn names_very_long() {
    let p = tmp_path("n_long");
    protocol::create(&p).unwrap();
    let name = "f".repeat(10_000);
    protocol::append(&p, b"L", &name).unwrap();
    assert_eq!(protocol::read(&p, &name).unwrap(), b"L");
    cleanup(&p);
}

#[test]
fn empty_name_still_roundtrips() {
    let p = tmp_path("n_empty");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"E", "").unwrap();
    assert_eq!(protocol::read(&p, "").unwrap(), b"E");
    cleanup(&p);
}

#[test]
fn duplicate_names_read_returns_oldest() {
    let p = tmp_path("n_dup");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"FIRST", "dup").unwrap();
    protocol::append(&p, b"SECOND", "dup").unwrap();
    assert_eq!(protocol::read(&p, "dup").unwrap(), b"FIRST");
    cleanup(&p);
}

#[test]
fn name_shares_prefix_with_other() {
    let p = tmp_path("n_pref");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"A", "note").unwrap();
    protocol::append(&p, b"B", "note2").unwrap();
    assert_eq!(protocol::read(&p, "note").unwrap(), b"A");
    assert_eq!(protocol::read(&p, "note2").unwrap(), b"B");
    cleanup(&p);
}

// ============================================================
// get_rdd
// ============================================================

#[test]
fn get_rdd_after_create_is_empty() {
    let p = tmp_path("rdd0");
    protocol::create(&p).unwrap();
    assert!(protocol::get_rdd(&p).unwrap().is_empty());
    cleanup(&p);
}

#[test]
fn get_rdd_order_matters_100() {
    let p = tmp_path("rdd100");
    protocol::create(&p).unwrap();
    for i in 0..100 {
        protocol::append(&p, b"x", &format!("entry-{:03}", i)).unwrap();
    }
    let r = protocol::get_rdd(&p).unwrap();
    assert_eq!(r.len(), 100);
    for (i, name) in r.iter().enumerate() {
        assert_eq!(name, &format!("entry-{:03}", i));
    }
    cleanup(&p);
}

#[test]
fn get_rdd_and_read_consistent_many() {
    let p = tmp_path("rdd_cons");
    protocol::create(&p).unwrap();
    for i in 0..30 {
        let d = format!("payload {}", i).into_bytes();
        protocol::append(&p, &d, &format!("f of {}", i)).unwrap();
    }
    for name in protocol::get_rdd(&p).unwrap() {
        // имя должно существовать и читаться
        protocol::read(&p, &name).unwrap();
    }
    cleanup(&p);
}

// ============================================================
// ошибки и мусорные файлы
// ============================================================

#[test]
fn ops_on_missing_file_error() {
    let p = tmp_path("missing_file_that_never_exists");
    let _ = std::fs::remove_file(&p);
    assert!(protocol::append(&p, b"d", "n").is_err());
    assert!(protocol::read(&p, "n").is_err());
    assert!(protocol::get_rdd(&p).is_err());
}

#[test]
fn read_on_create_only_archive_finds_nothing() {
    let p = tmp_path("fresh");
    protocol::create(&p).unwrap();
    assert!(protocol::read(&p, "any").is_err());
    cleanup(&p);
}

#[test]
fn garbage_file_generated_from_empty_content_is_rejected() {
    let p = tmp_path("empty_file");
    std::fs::write(&p, b"").unwrap();
    assert!(protocol::append(&p, b"d", "n").is_err());
    cleanup(&p);
}

#[test]
fn file_without_newline_is_rejected() {
    let p = tmp_path("no_nl");
    std::fs::write(&p, b"no newline in here at all").unwrap();
    assert!(protocol::get_rdd(&p).is_err());
    cleanup(&p);
}

#[test]
fn file_with_only_newline_is_rejected() {
    let p = tmp_path("only_nl");
    std::fs::write(&p, b"\n").unwrap();
    assert!(protocol::get_rdd(&p).is_err());
    assert!(protocol::read(&p, "x").is_err());
    cleanup(&p);
}

#[test]
fn truncated_to_single_byte_is_rejected() {
    let p = tmp_path("one_byte");
    std::fs::write(&p, b"5").unwrap();
    assert!(protocol::get_rdd(&p).is_err());
    cleanup(&p);
}

#[test]
fn corrupted_eob_number_is_rejected() {
    let p = tmp_path("bad_eob");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"abc", "f").unwrap();
    let mut s = std::fs::read_to_string(&p).unwrap();
    // затираем число eob в последней строке
    let idx = s.rfind('\n').unwrap();
    s.truncate(idx + 1);
    s.push_str("NaN");
    std::fs::write(&p, s).unwrap();
    assert!(protocol::get_rdd(&p).is_err());
    cleanup(&p);
}

#[test]
fn eob_out_of_bounds_quirk_silently_yields_empty_rdd() {
    // Задокументированная причуда (баг-кандидат): число eob не сверяется
    // с длиной файла. Раздутый eob -> seek за границу читает 0 байт -> get_rdd = []
    // вместо ошибки. Данные не теряются (лежат до rdd-региона), таблица да.
    let p = tmp_path("eob_far");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"abc", "f").unwrap();
    let s = std::fs::read_to_string(&p).unwrap();
    let big = format!("{}99999999", s);
    std::fs::write(&p, big).unwrap();
    let r = protocol::get_rdd(&p).unwrap();
    assert!(r.is_empty());
    cleanup(&p);
}

#[test]
fn corrupted_file_variants_selected_as_error_or_inconsistent() {
    let p = tmp_path("trunc_mid");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"ABCDEF", "f").unwrap();
    protocol::append(&p, b"XY", "g").unwrap();
    let bytes = std::fs::read(&p).unwrap();
    // обрезаем хвост (eob-строку) — вся цепочка обязана отказаться работать
    std::fs::write(&p, &bytes[..bytes.len() - 2]).unwrap();
    assert!(protocol::read(&p, "f").is_err());
    assert!(protocol::get_rdd(&p).is_err());
    cleanup(&p);
}

#[test]
fn read_missing_name_on_valid_archive() {
    let p = tmp_path("r_missing");
    protocol::create(&p).unwrap();
    protocol::append(&p, b"x", "known").unwrap();
    assert!(protocol::read(&p, "unknown").is_err());
    assert!(protocol::read(&p, "").is_err()); // пустое имя не создано
    cleanup(&p);
}

// ============================================================
// большой стресс: 300 append'ов, полный обход
// ============================================================

#[test]
fn stress_300_appends_and_full_traversal() {
    let p = tmp_path("stress300");
    protocol::create(&p).unwrap();
    for i in 0..300 {
        let d = format!("body[{}.pad-{}]", i, i % 37).into_bytes();
        protocol::append(&p, &d, &format!("k/{:04}/v", i)).unwrap();
    }
    let rdd = protocol::get_rdd(&p).unwrap();
    assert_eq!(rdd.len(), 300);
    for i in 0..300 {
        let name = format!("k/{:04}/v", i);
        assert_eq!(
            protocol::read(&p, &name).unwrap(),
            format!("body[{}.pad-{}]", i, i % 37).into_bytes(),
            "mismatch at {}",
            i
        );
    }
    cleanup(&p);
}

#[test]
fn interleave_read_write_cycles() {
    let p = tmp_path("interleave");
    protocol::create(&p).unwrap();
    for cycle in 0..5 {
        for j in 0..10 {
            let name = format!("c{}-f{}", cycle, j);
            let d = format!("cycle-{}-file-{}", cycle, j).into_bytes();
            protocol::append(&p, &d, &name).unwrap();
        }
        // полный обход после каждого цикла
        for c in 0..=cycle {
            for j in 0..10 {
                let name = format!("c{}-f{}", c, j);
                assert_eq!(
                    protocol::read(&p, &name).unwrap(),
                    format!("cycle-{}-file-{}", c, j).into_bytes()
                );
            }
        }
    }
    cleanup(&p);
}

#[test]
fn archive_size_is_linear_in_entries() {
    let p = tmp_path("linear");
    protocol::create(&p).unwrap();
    for i in 0..50 {
        protocol::append(&p, b"stable-body", &format!("e{}", i)).unwrap();
    }
    let size = std::fs::metadata(&p).unwrap().len();
    // 50 записей по 11 байт данных: суммарный рост должен быть O(n), не квадратичным
    let overhead = size - 50 * 11;
    assert!(
        overhead < 50 * 250,
        "overhead per entry too big: {} bytes/entry",
        overhead / 50
    );
    cleanup(&p);
}

// ============================================================
// CLI: subprocess-тесты
// ============================================================
#[test]
fn cli_no_args_bails() {
    let r = run(&[]);
    assert_ne!(r.code, 0);
    assert!(!r.stderr.is_empty(), "должен быть usage/ошибка");
}

#[test]
fn cli_unknown_flag_bails() {
    let a = tmp_path("cli_unk_flag");
    let r = run(&[a.to_str().unwrap(), "--frobnicate"]);
    assert_ne!(r.code, 0);
    cleanup(&a);
}

#[test]
fn cli_second_positional_bails() {
    let a = tmp_path("cli_dup");
    let b = tmp_path("cli_dup2");
    let r = run(&[a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_ne!(r.code, 0);
    cleanup(&a);
    cleanup(&b);
}

#[test]
fn cli_conflicting_actions_bails() {
    let a = tmp_path("cli_conflict");
    let r = run(&[a.to_str().unwrap(), "--list", "--append"]);
    assert_ne!(r.code, 0);
    cleanup(&a);
}

#[test]
fn cli_help_exits_zero_and_prints_usage() {
    let r = run(&["--help"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("Usage"), "help: {}", r.stdout);
    assert!(
        r.stdout.contains("ARCHIVE") && r.stdout.contains("--list"),
        "help: {}",
        r.stdout
    );
}

#[test]
fn cli_bare_path_creates_archive_and_lists_exit_zero() {
    let p = tmp_path("cli_create");
    let _ = std::fs::remove_file(&p);
    let r = run(&[p.to_str().unwrap()]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert_eq!(std::fs::read(&p).unwrap(), b"\n0");
    assert_eq!(r.stdout.trim(), "[]");
    cleanup(&p);
}

#[test]
fn cli_list_on_fresh_archive_is_empty() {
    let p = tmp_path("cli_list0");
    let _ = std::fs::remove_file(&p);
    let r = run(&[p.to_str().unwrap(), "--list"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), "[]");
    cleanup(&p);
}

#[test]
fn cli_upload_then_list_then_read() {
    let p = tmp_path("cli_up");
    let f = tmp_path("cli_up_src");
    std::fs::write(&f, b"note body\nsecond line\n").unwrap();
    let _ = std::fs::remove_file(&p);

    let r = run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "note.txt"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);

    let r = run(&[p.to_str().unwrap(), "--list"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("note.txt"), "list: {}", r.stdout);

    let r = run(&[p.to_str().unwrap(), "--read", "note.txt"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert_eq!(r.stdout, "note body\nsecond line\n");

    cleanup(&p);
    cleanup(&f);
}

#[test]
fn cli_upload_rejects_empty_file() {
    let p = tmp_path("cli_empty");
    let f = tmp_path("cli_empty_src");
    std::fs::write(&f, b"").unwrap();
    let _ = std::fs::remove_file(&p);
    let r = run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "empty.bin"]);
    assert_ne!(r.code, 0);
    assert!(r.stderr.contains("null lengh"), "stderr: {}", r.stderr);
    cleanup(&p);
    cleanup(&f);
}

#[test]
fn cli_upload_same_name_twice_bails() {
    let p = tmp_path("cli_dupname");
    let f = tmp_path("cli_dupname_src");
    std::fs::write(&f, b"stuff").unwrap();
    let _ = std::fs::remove_file(&p);
    run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "dup"]);
    let r = run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "dup"]);
    assert_ne!(r.code, 0);
    assert!(r.stderr.contains("same name"), "stderr: {}", r.stderr);
    cleanup(&p);
    cleanup(&f);
}

#[test]
fn cli_upload_binary_file_roundtrip() {
    let p = tmp_path("cli_bin");
    let f = tmp_path("cli_bin_src");
    let data: Vec<u8> = (0u8..=255).collect();
    std::fs::write(&f, &data).unwrap();
    let _ = std::fs::remove_file(&p);

    run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "bin"]);
    let r = run(&[p.to_str().unwrap(), "--read", "bin"]);
    assert_eq!(r.code, 0);
    assert_eq!(r.raw_stdout, data);
    cleanup(&p);
    cleanup(&f);
}

#[test]
fn cli_upload_name_differs_from_source_path() {
    // смысл двухзначного --upload: путь на диске и имя в архиве — независимы
    let p = tmp_path("cli_names");
    let f = tmp_path("some totally different file.txt");
    std::fs::write(&f, b"renamed body").unwrap();
    let _ = std::fs::remove_file(&p);
    run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "journal.txt"]);
    let list = run(&[p.to_str().unwrap(), "--list"]).stdout;
    assert!(list.contains("journal.txt"), "{}", list);
    assert!(!list.contains(f.to_str().unwrap()), "{}", list);
    assert_eq!(
        run(&[p.to_str().unwrap(), "--read", "journal.txt"]).stdout,
        "renamed body"
    );
    cleanup(&p);
    cleanup(&f);
}

#[test]
fn cli_upload_requires_two_values() {
    let p = tmp_path("cli_need2");
    let f = tmp_path("cli_need2_src");
    std::fs::write(&f, b"x").unwrap();
    let _ = std::fs::remove_file(&p);
    // одно значение — clap-ошибка
    let r = run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap()]);
    assert_ne!(r.code, 0, "stderr: {}", r.stderr);
    // ноль значений — тоже
    let r = run(&[p.to_str().unwrap(), "--upload"]);
    assert_ne!(r.code, 0);
    cleanup(&p);
    cleanup(&f);
}

#[test]
fn cli_read_missing_bails() {
    let p = tmp_path("cli_noread");
    run(&[p.to_str().unwrap()]);
    let r = run(&[p.to_str().unwrap(), "--read", "ghost"]);
    assert_ne!(r.code, 0);
    assert!(
        r.stderr.contains("not found in rdd") || !r.stderr.is_empty(),
        "stderr: {}",
        r.stderr
    );
    cleanup(&p);
}

#[test]
fn cli_archive_is_reusable_across_processes() {
    let p = tmp_path("cli_reuse");
    let f1 = tmp_path("cli_reuse1");
    let f2 = tmp_path("cli_reuse2");
    std::fs::write(&f1, b"first-note").unwrap();
    std::fs::write(&f2, b"second-note").unwrap();
    let _ = std::fs::remove_file(&p);

    run(&[p.to_str().unwrap(), "--upload", f1.to_str().unwrap(), "n1"]);
    run(&[p.to_str().unwrap(), "--upload", f2.to_str().unwrap(), "n2"]);

    let r = run(&[p.to_str().unwrap(), "--list"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.contains("n1"), "{}", r.stdout);
    assert!(r.stdout.contains("n2"), "{}", r.stdout);

    assert_eq!(
        run(&[p.to_str().unwrap(), "--read", "n1"]).stdout,
        "first-note"
    );
    assert_eq!(
        run(&[p.to_str().unwrap(), "--read", "n2"]).stdout,
        "second-note"
    );

    cleanup(&p);
    cleanup(&f1);
    cleanup(&f2);
}

#[test]
fn cli_upload_read_data_with_newlines_and_null() {
    let p = tmp_path("cli_nl");
    let f = tmp_path("cli_nl_src");
    let data: Vec<u8> = b"line1\n\r\n\tline with \x00 null and end".to_vec();
    std::fs::write(&f, &data).unwrap();
    let _ = std::fs::remove_file(&p);
    run(&[p.to_str().unwrap(), "--upload", f.to_str().unwrap(), "weird"]);
    let r = run(&[p.to_str().unwrap(), "--read", "weird"]);
    assert_eq!(r.raw_stdout, data);
    cleanup(&p);
    cleanup(&f);
}
