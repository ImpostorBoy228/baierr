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

pub fn oneshot_append<P: AsRef<std::path::Path>>(
    path: P,
    data: &[u8],
    inner_path: &str,
) -> Result<()> {
    let path = path.as_ref();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
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
        std::env::temp_dir().join(format!("baierr_{}_{}_{:?}_{}.b", name, nanos, tid, std::process::id()))
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
        oneshot_append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.starts_with("\nfoo 3\nABC\n"), "got: {:?}", s);
        assert!(s.contains("foo 1"), "got: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_empty_data() {
        let p = tmp_path("empty");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"", "empty").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("empty 0\n\n"), "got: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn multiple_appends_three() {
        let p = tmp_path("multi3");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        oneshot_append(p.to_str().unwrap(), b"XY", "bar").unwrap();
        oneshot_append(p.to_str().unwrap(), b"Z", "baz").unwrap();
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
        oneshot_append(p.to_str().unwrap(), data, "nl").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(bytes.windows(data.len()).any(|w| w == data), "data not found");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_large_data() {
        let p = tmp_path("large");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![b'x'; 100_000];
        oneshot_append(p.to_str().unwrap(), &data, "big").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(bytes.len() > 100_000);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn inner_path_with_spaces() {
        let p = tmp_path("spaces");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"Q", "my file.txt").unwrap();
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
        oneshot_append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        // find entry line "foo 1"
        let entry = s.lines().find(|l| l.starts_with("foo ") && !l.starts_with("foo 3")).unwrap();
        let off: usize = entry.split_whitespace().nth(1).unwrap().parse().unwrap();
        // bytes at off should start with "foo 3"
        let bytes = fs::read(&p).unwrap();
        assert!(bytes[off..].starts_with(b"foo 3"), "off={}, bytes={:?}", off, &bytes[off..off+5]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_to_empty_file_errors() {
        let p = tmp_path("emptyfile");
        fs::write(&p, b"").unwrap();
        let res = oneshot_append(p.to_str().unwrap(), b"X", "f");
        assert!(res.is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_to_garbage_errors() {
        let p = tmp_path("garbage");
        fs::write(&p, b"no newline here").unwrap();
        let res = oneshot_append(p.to_str().unwrap(), b"X", "f");
        assert!(res.is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn second_append_preserves_first_data() {
        let p = tmp_path("preserve");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"FIRST", "a").unwrap();
        oneshot_append(p.to_str().unwrap(), b"SECOND", "b").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(bytes.windows(5).any(|w| w == b"FIRST"));
        assert!(bytes.windows(6).any(|w| w == b"SECOND"));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn eob_last_line_parseable() {
        let p = tmp_path("eob");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        oneshot_append(p.to_str().unwrap(), b"XY", "bar").unwrap();
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
            oneshot_append(p.to_str().unwrap(), &data, &path).unwrap();
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
        assert!(oneshot_append(p.to_str().unwrap(), b"x", "y").is_ok());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn root_desc_contains_all_entries() {
        let p = tmp_path("rootall");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"a", "one").unwrap();
        oneshot_append(p.to_str().unwrap(), b"b", "two").unwrap();
        oneshot_append(p.to_str().unwrap(), b"c", "three").unwrap();
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
        oneshot_append(p.to_str().unwrap(), b"a", "one").unwrap();
        oneshot_append(p.to_str().unwrap(), b"b", "two").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(!s.contains("\n\n\n"), "too many blanks: {:?}", s);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn file_size_grows() {
        let p = tmp_path("grow");
        create(p.to_str().unwrap()).unwrap();
        let before = fs::metadata(&p).unwrap().len();
        oneshot_append(p.to_str().unwrap(), b"ABC", "foo").unwrap();
        let after = fs::metadata(&p).unwrap().len();
        assert!(after > before);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_different_inner_paths() {
        let p = tmp_path("paths");
        create(p.to_str().unwrap()).unwrap();
        oneshot_append(p.to_str().unwrap(), b"1", "alpha").unwrap();
        oneshot_append(p.to_str().unwrap(), b"2", "beta").unwrap();
        oneshot_append(p.to_str().unwrap(), b"3", "gamma").unwrap();
        let s = fs::read_to_string(&p).unwrap();
        assert!(s.contains("alpha 1"));
        assert!(s.contains("beta "));
        assert!(s.contains("gamma "));
        let _ = fs::remove_file(&p);
    }
}