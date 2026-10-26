use anyhow::Result;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
};

/* file structure:
 * [local header]
 * [data]
 * [local header 2]
 * [data 2]
 * ...
 * [root directory description]
 *      * file -> offset of local header
 *      * ...
 * [eob] -- offset of root directory description
 */
pub fn create(path: &str) -> Result<()> {
    fs::write(path, "\n0")?; // спижжено
    Ok(())
}

pub fn append<P: AsRef<std::path::Path>>(path: P, data: &[u8], inner_path: &str) -> Result<()> {
    let path = path.as_ref();
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let (eof, eob) = get_last_line_position_and_value(&mut file)?;
    let eob_offset: u64 = eob.parse().map_err(|_| anyhow::anyhow!("invalid eob"))?;
    file.seek(SeekFrom::Start(eob_offset))?;
    let rdd_len = eof.saturating_sub(eob_offset).saturating_sub(1);
    let mut rdd: Vec<u8> = vec![0; rdd_len as usize];
    if rdd_len > 0 {
        file.read_exact(&mut rdd)?;
    }
    // '\n' guaranteed
    file.seek(SeekFrom::Start(eof))?;
    file.write_all(format!("{} {}\n", inner_path, data.len()).as_bytes())?;
    file.write_all(data)?;
    file.write_all(b"\n")?; // here it appears
    let eob_new = file.stream_position()?;
    file.write_all(&rdd)?;
    if !rdd.is_empty() && !rdd.ends_with(b"\n") {
        file.write_all(b"\n")?;
    }
    file.write_all(format!("{} {}", inner_path, eof).as_bytes())?;
    file.write_all(format!("\n {}", eob_new).as_bytes())?;
    Ok(())
}

pub fn get_rdd<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<String>> {
    let path = path.as_ref();
    let mut file = OpenOptions::new().read(true).open(path)?;
    let (eof, eob) = get_last_line_position_and_value(&mut file)?;
    let eob_offset: u64 = eob.parse().map_err(|_| anyhow::anyhow!("invalid eob"))?;
    file.seek(SeekFrom::Start(eob_offset))?;
    let rdd_len = eof.saturating_sub(eob_offset).saturating_sub(1);
    let mut rdd: Vec<u8> = vec![0; rdd_len as usize];
    if rdd_len > 0 {
        file.read_exact(&mut rdd)?;
    }
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(&rdd).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (name, _) = line
            .rsplit_once(' ')
            .ok_or_else(|| anyhow::anyhow!("bad rdd entry"))?;
        out.push(name.to_string());
    }
    Ok(out)
}

pub fn read<P: AsRef<std::path::Path>>(path: P, inner_path: &str) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let mut file = OpenOptions::new().read(true).open(path)?;
    let (eof, eob) = get_last_line_position_and_value(&mut file)?;
    let eob_offset: u64 = eob.parse().map_err(|_| anyhow::anyhow!("invalid eob"))?;
    file.seek(SeekFrom::Start(eob_offset))?;
    let rdd_len = eof.saturating_sub(eob_offset).saturating_sub(1);
    let mut rdd: Vec<u8> = vec![0; rdd_len as usize];
    if rdd_len > 0 {
        file.read_exact(&mut rdd)?;
    }

    let mut offset: Option<u64> = None;
    for line in String::from_utf8_lossy(&rdd).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (name, off) = line
            .rsplit_once(' ')
            .ok_or_else(|| anyhow::anyhow!("bad rdd entry"))?;
        if name == inner_path {
            offset = Some(off.parse().map_err(|_| anyhow::anyhow!("bad offset"))?);
            break;
        }
    }
    let offset = offset.ok_or_else(|| anyhow::anyhow!("inner_path not found in rdd"))?;

    file.seek(SeekFrom::Start(offset))?;
    let mut header = Vec::new();
    loop {
        let mut b = [0u8; 1];
        file.read_exact(&mut b)?;
        if b[0] == b'\n' {
            break;
        }
        header.push(b[0]);
    }
    let header = String::from_utf8(header).map_err(|_| anyhow::anyhow!("invalid header"))?;
    let (name, size_str) = header
        .rsplit_once(' ')
        .ok_or_else(|| anyhow::anyhow!("bad header"))?;
    if name != inner_path {
        return Err(anyhow::anyhow!("inner_path mismatch"));
    }
    let size: usize = size_str.parse().map_err(|_| anyhow::anyhow!("bad size"))?;
    let mut data = vec![0; size];
    if size > 0 {
        file.read_exact(&mut data)?;
    }
    Ok(data)
}

fn get_last_line_position_and_value(file: &mut File) -> Result<(u64, String)> {
    let file_len = file.metadata()?.len();
    if file_len == 0 {
        return Err(anyhow::anyhow!("len 0"));
    }

    let mut cursor = file_len;
    let mut buf = [0; 1];

    while cursor > 0 {
        cursor -= 1;
        file.seek(SeekFrom::Start(cursor))?;
        file.read_exact(&mut buf)?;

        if buf == [b'\n'] && cursor < file_len - 1 {
            file.seek(SeekFrom::Start(cursor + 1))?;
            let mut last = String::new();
            file.read_to_string(&mut last)?;
            return Ok((cursor + 1, last.trim().to_string()));
        }
    }

    Err(anyhow::anyhow!("one \\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tid = std::thread::current().id();
        std::env::temp_dir().join(format!(
            "baierr_{}_{}_{:?}_{}.b",
            name,
            nanos,
            tid,
            std::process::id()
        ))
    }

    #[test]
    fn create_writes_initial_marker() {
        let p = tmp_path("create");
        create(p.to_str().unwrap()).unwrap();
        let bytes = fs::read(&p).unwrap();
        assert_eq!(bytes, b"\n0");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn create_overwrites_existing() {
        let p = tmp_path("create_overwrite");
        fs::write(&p, b"garbage").unwrap();
        create(p.to_str().unwrap()).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"\n0");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn single_append_layout() {
        let p = tmp_path("single");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.starts_with("\nfoo 3\nABC\n"), "got: {:?}", s);
        assert!(s.contains("foo 1"), "got: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_empty_data() {
        let p = tmp_path("empty");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"", "empty").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("empty 0\n\n"), "got: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn multiple_appends_three() {
        let p = tmp_path("multi3");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        append(p.to_str().unwrap(), b"XY", "bar").unwrap();
        append(p.to_str().unwrap(), b"Z", "baz").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        // each root entry present and separated
        assert!(s.contains("foo 1\n"), "got: {:?}", s);
        assert!(s.contains("bar 17\n"), "got: {:?}", s);
        assert!(s.contains("baz 39"), "got: {:?}", s);
        // no merged entries
        assert!(!s.contains("foo 1bar"), "got: {:?}", s);
        assert!(!s.contains("bar 17baz"), "got: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_data_with_newlines() {
        let p = tmp_path("nl");
        create(p.to_str().unwrap()).unwrap();
        let data = b"a\nb\nc";
        append(p.to_str().unwrap(), data, "nl").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(
            bytes.windows(data.len()).any(|w| w == data),
            "data not found"
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_large_data() {
        let p = tmp_path("large");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![b'x'; 100_000];
        append(p.to_str().unwrap(), &data, "big").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(bytes.len() > 100_000);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn inner_path_with_spaces() {
        let p = tmp_path("spaces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"Q", "my file.txt").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("my file.txt 1"), "got: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn root_offset_points_to_header() {
        // After one append, root entry offset (second token of entry) should
        // point at the local header line for that file.
        let p = tmp_path("offset");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        // find entry line "foo 1"
        let entry = s
            .lines()
            .find(|l| l.starts_with("foo ") && !l.starts_with("foo 3"))
            .unwrap();
        let off: usize = entry.split_whitespace().nth(1).unwrap().parse().unwrap();
        // bytes at off should start with "foo 3"
        let bytes = fs::read(&p).unwrap();
        assert!(
            bytes[off..].starts_with(b"foo 3"),
            "off={}, bytes={:?}",
            off,
            &bytes[off..off + 5]
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_to_empty_file_errors() {
        let p = tmp_path("emptyfile");
        fs::write(&p, b"").unwrap();
        let res = append(p.to_str().unwrap(), b"X", "f");
        assert!(res.is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_to_garbage_errors() {
        let p = tmp_path("garbage");
        fs::write(&p, b"no newline here").unwrap();
        let res = append(p.to_str().unwrap(), b"X", "f");
        assert!(res.is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn second_append_preserves_first_data() {
        let p = tmp_path("preserve");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"FIRST", "a").unwrap();
        append(p.to_str().unwrap(), b"SECOND", "b").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(bytes.windows(5).any(|w| w == b"FIRST"));
        assert!(bytes.windows(6).any(|w| w == b"SECOND"));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn eob_last_line_parseable() {
        let p = tmp_path("eob");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        append(p.to_str().unwrap(), b"XY", "bar").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        let last = s.lines().last().unwrap();
        let v: u64 = last.trim().parse().unwrap();
        assert!(v > 0);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn many_appends_stress() {
        let p = tmp_path("stress");
        create(p.to_str().unwrap()).unwrap();
        for i in 0..50 {
            let data = format!("data{}", i).into_bytes();
            let path = format!("f{}", i);
            append(p.to_str().unwrap(), &data, &path).unwrap();
        }
        let s = fs::read_to_string(&p).unwrap();
        for i in 0..50 {
            assert!(s.contains(&format!("f{} ", i)), "missing f{}", i);
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn create_then_immediate_read() {
        let p = tmp_path("imm");
        create(p.to_str().unwrap()).unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert_eq!(s, "\n0");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_returns_ok() {
        let p = tmp_path("ok");
        create(p.to_str().unwrap()).unwrap();
        assert!(append(p.to_str().unwrap(), b"x", "y").is_ok());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn root_desc_contains_all_entries() {
        let p = tmp_path("rootall");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"a", "one").unwrap();
        append(p.to_str().unwrap(), b"b", "two").unwrap();
        append(p.to_str().unwrap(), b"c", "three").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("one 1"));
        assert!(s.contains("two "));
        assert!(s.contains("three "));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn no_double_blank_lines() {
        let p = tmp_path("noblank");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"a", "one").unwrap();
        append(p.to_str().unwrap(), b"b", "two").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(!s.contains("\n\n\n"), "too many blanks: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn file_size_grows() {
        let p = tmp_path("grow");
        create(p.to_str().unwrap()).unwrap();
        let before = fs::metadata(&p).unwrap().len();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let after = fs::metadata(&p).unwrap().len();
        assert!(after > before);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn get_rdd_empty() {
        let p = tmp_path("rddempty");
        create(p.to_str().unwrap()).unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert!(r.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn get_rdd_lists_paths() {
        let p = tmp_path("rddlist");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"a", "one").unwrap();
        append(p.to_str().unwrap(), b"b", "two").unwrap();
        append(p.to_str().unwrap(), b"c", "three").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r, vec!["one", "two", "three"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_roundtrip_single() {
        let p = tmp_path("readsingle");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"hello world", "foo").unwrap();
        let data = read(p.to_str().unwrap(), "foo").unwrap();
        assert_eq!(data, b"hello world");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_roundtrip_empty() {
        let p = tmp_path("readempty");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"", "empty").unwrap();
        let data = read(p.to_str().unwrap(), "empty").unwrap();
        assert!(data.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_multiple_files() {
        let p = tmp_path("readmulti");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"AAA", "a").unwrap();
        append(p.to_str().unwrap(), b"BBB", "b").unwrap();
        append(p.to_str().unwrap(), b"CCC", "c").unwrap();
        assert_eq!(read(p.to_str().unwrap(), "a").unwrap(), b"AAA");
        assert_eq!(read(p.to_str().unwrap(), "b").unwrap(), b"BBB");
        assert_eq!(read(p.to_str().unwrap(), "c").unwrap(), b"CCC");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_with_newlines() {
        let p = tmp_path("readnl");
        create(p.to_str().unwrap()).unwrap();
        let data = b"x\ny\nz";
        append(p.to_str().unwrap(), data, "nl").unwrap();
        assert_eq!(read(p.to_str().unwrap(), "nl").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_large() {
        let p = tmp_path("readlarge");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![b'q'; 50_000];
        append(p.to_str().unwrap(), &data, "big").unwrap();
        assert_eq!(read(p.to_str().unwrap(), "big").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_missing_path_errors() {
        let p = tmp_path("readmissing");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"x", "a").unwrap();
        assert!(read(p.to_str().unwrap(), "nope").is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_path_with_spaces() {
        let p = tmp_path("readspaces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"SP", "my file.txt").unwrap();
        assert_eq!(read(p.to_str().unwrap(), "my file.txt").unwrap(), b"SP");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn get_rdd_with_spaces() {
        let p = tmp_path("rddspaces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"SP", "my file.txt").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r, vec!["my file.txt"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn concurrency_appends() {
        use std::thread;
        let p = tmp_path("conc");
        create(p.to_str().unwrap()).unwrap();
        let p = std::sync::Arc::new(p);
        let mut handles = Vec::new();
        for i in 0..8 {
            let p = p.clone();
            handles.push(thread::spawn(move || {
                let data = format!("t{}", i).into_bytes();
                let name = format!("t{}", i);
                append(p.to_str().unwrap(), &data, &name).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // best-effort: at least one entry present
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert!(!r.is_empty());
        let _ = fs::remove_file(p.as_path());
    }

    #[test]
    fn duplicate_inner_path_read_first() {
        let p = tmp_path("dup");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"FIRST", "dup").unwrap();
        append(p.to_str().unwrap(), b"SECOND", "dup").unwrap();
        // read should find first occurrence
        let d = read(p.to_str().unwrap(), "dup").unwrap();
        assert_eq!(d, b"FIRST");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn empty_inner_path() {
        let p = tmp_path("emptyname");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"X", "").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert!(r.contains(&String::new()));
        assert_eq!(read(p.to_str().unwrap(), "").unwrap(), b"X");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn unicode_inner_path() {
        let p = tmp_path("unicode");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"U", "файл/😀").unwrap();
        assert_eq!(read(p.to_str().unwrap(), "файл/😀").unwrap(), b"U");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn data_with_null_bytes() {
        let p = tmp_path("nulls");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![0u8, 1, 2, 0, 3];
        append(p.to_str().unwrap(), &data, "n").unwrap();
        assert_eq!(read(p.to_str().unwrap(), "n").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn pathbuf_api() {
        let p = tmp_path("pathbuf");
        create(p.to_str().unwrap()).unwrap();
        let pb = std::path::PathBuf::from(&p);
        append(&pb, b"P", "pb").unwrap();
        assert_eq!(read(&pb, "pb").unwrap(), b"P");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn many_small_appends() {
        let p = tmp_path("manysmall");
        create(p.to_str().unwrap()).unwrap();
        for i in 0..200 {
            append(p.to_str().unwrap(), b"x", &format!("s{}", i)).unwrap();
        }
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r.len(), 200);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn get_rdd_order_preserved() {
        let p = tmp_path("order");
        create(p.to_str().unwrap()).unwrap();
        for i in 0..10 {
            append(p.to_str().unwrap(), b"x", &format!("o{}", i)).unwrap();
        }
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        for i in 0..10 {
            assert_eq!(r[i], format!("o{}", i));
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_all_entries_after_many() {
        let p = tmp_path("readall");
        create(p.to_str().unwrap()).unwrap();
        for i in 0..50 {
            let data = format!("v{}", i).into_bytes();
            append(p.to_str().unwrap(), &data, &format!("k{}", i)).unwrap();
        }
        for i in 0..50 {
            assert_eq!(
                read(p.to_str().unwrap(), &format!("k{}", i)).unwrap(),
                format!("v{}", i).into_bytes()
            );
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn truncated_file_errors() {
        let p = tmp_path("trunc");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        // truncate last few bytes
        let bytes = fs::read(&p).unwrap();
        fs::write(&p, &bytes[..bytes.len() - 3]).unwrap();
        assert!(read(p.to_str().unwrap(), "foo").is_err() || get_rdd(p.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn corrupted_eob_errors() {
        let p = tmp_path("corrupteob");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let mut s = fs::read_to_string(&p).unwrap();
        // replace last line number with garbage
        if let Some(idx) = s.rfind('\n') {
            s = s[..idx + 1].to_string() + "NOTANUMBER";
        }
        fs::write(&p, &s).unwrap();
        assert!(get_rdd(p.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_different_inner_paths() {
        let p = tmp_path("paths");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), b"1", "alpha").unwrap();
        append(p.to_str().unwrap(), b"2", "beta").unwrap();
        append(p.to_str().unwrap(), b"3", "gamma").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("alpha 1"));
        assert!(s.contains("beta "));
        assert!(s.contains("gamma "));
        let _ = fs::remove_file(&p);
    }
}
