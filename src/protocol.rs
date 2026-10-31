use anyhow::{Result, bail};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
};

pub const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const OVERHEAD: usize = NONCE_LEN + TAG_LEN;

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
pub fn create<P: AsRef<std::path::Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    fs::write(path, "\n0")?; // спижжено
    Ok(())
}

pub fn append<P: AsRef<std::path::Path>>(
    path: P,
    key: &[u8; KEY_LEN],
    data: &[u8],
    inner_path: &str,
) -> Result<()> {
    let path = path.as_ref();
    let ciphertext = encrypt(key, data)?;
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
    file.write_all(format!("{} {}\n", inner_path, ciphertext.len()).as_bytes())?;
    file.write_all(&ciphertext)?;
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

pub fn read<P: AsRef<std::path::Path>>(
    path: P,
    key: &[u8; KEY_LEN],
    inner_path: &str,
) -> Result<Vec<u8>> {
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
    if size < OVERHEAD {
        bail!("payload too small: {}", size);
    }
    let mut payload = vec![0; size];
    file.read_exact(&mut payload)?;
    decrypt(key, &payload)
}

fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce_bytes)?;
    let nonce = XNonce::try_from(&nonce_bytes as &[u8]).expect("nonce len checked");
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encrypt failed: {e}"))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn decrypt(key: &[u8; KEY_LEN], payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() < OVERHEAD {
        bail!("payload too small: {}", payload.len());
    }
    let (nonce, ct) = payload.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::try_from(nonce as &[u8]).expect("nonce len fixed");
    cipher
        .decrypt(&nonce, ct)
        .map_err(|_| anyhow::anyhow!("wrong key or corrupted data"))
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
            let mut last = Vec::new();
            file.read_to_end(&mut last)?;
            return Ok((
                cursor + 1,
                String::from_utf8_lossy(&last).trim().to_string(),
            ));
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

    const KEY: [u8; 32] = [0x2a; 32];

    fn parse_rdd_from_bytes(bytes: &[u8]) -> Vec<(String, usize)> {
        let idx_last_nl = bytes.iter().rposition(|&b| b == b'\n').unwrap();
        let eob: usize = String::from_utf8_lossy(&bytes[idx_last_nl + 1..])
            .trim()
            .parse()
            .unwrap();
        let rdd = &bytes[eob..idx_last_nl];
        let mut out = Vec::new();
        for line in String::from_utf8_lossy(rdd).lines() {
            if line.trim().is_empty() {
                continue;
            }
            let (name, off) = line.rsplit_once(' ').unwrap();
            if let Ok(off) = off.parse::<usize>() {
                out.push((name.to_string(), off));
            }
        }
        out
    }

    fn find_entry_offset(bytes: &[u8], name: &str) -> Option<usize> {
        parse_rdd_from_bytes(bytes)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| o)
    }

    fn ct_payload_ranges(bytes: &[u8]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (_, off) in parse_rdd_from_bytes(bytes) {
            let nl = bytes[off..].iter().position(|&b| b == b'\n').unwrap();
            let header = String::from_utf8_lossy(&bytes[off..off + nl]).into_owned();
            let size: usize = header.rsplit_once(' ').unwrap().1.parse().unwrap();
            out.push((off + nl + 1, off + nl + 1 + size));
        }
        out
    }

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
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(
            bytes.starts_with(b"\nfoo 43\n"),
            "got: {:?}",
            String::from_utf8_lossy(&bytes[..40])
        );
        let header_end = b"\nfoo 43\n".len();
        let ct_region = &bytes[header_end..header_end + 43];
        assert!(
            !ct_region.windows(3).any(|w| w == b"ABC"),
            "plaintext leaked"
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_empty_data() {
        let p = tmp_path("empty");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"", "empty").unwrap();
        let bytes = fs::read(&p).unwrap();
        // пустой plaintext => ровно 40 байт (nonce || tag), header "empty 40"
        assert!(
            bytes.starts_with(b"\nempty 40\n"),
            "got: {:?}",
            String::from_utf8_lossy(&bytes[..40])
        );
        assert_eq!(read(p.to_str().unwrap(), &KEY, "empty").unwrap(), b"");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn multiple_appends_three() {
        let p = tmp_path("multi3");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
        append(p.to_str().unwrap(), &KEY, b"XY", "bar").unwrap();
        append(p.to_str().unwrap(), &KEY, b"Z", "baz").unwrap();
        assert_eq!(
            read(p.to_str().unwrap(), &KEY, "foo").unwrap(),
            b"ABC",
            "foo roundtrip"
        );
        assert_eq!(read(p.to_str().unwrap(), &KEY, "bar").unwrap(), b"XY");
        assert_eq!(read(p.to_str().unwrap(), &KEY, "baz").unwrap(), b"Z");
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r, vec!["foo", "bar", "baz"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_data_with_newlines() {
        let p = tmp_path("nl");
        create(p.to_str().unwrap()).unwrap();
        let data = b"a\nb\nc";
        append(p.to_str().unwrap(), &KEY, data, "nl").unwrap();
        let bytes = fs::read(&p).unwrap();
        // шифротекст — больше не ищем plaintext в файле, только roundtrip
        assert!(
            !bytes.windows(data.len()).any(|w| w == data),
            "plaintext leaked"
        );
        assert_eq!(read(p.to_str().unwrap(), &KEY, "nl").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_large_data() {
        let p = tmp_path("large");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![b'x'; 100_000];
        append(p.to_str().unwrap(), &KEY, &data, "big").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(bytes.len() > 100_000);
        assert_eq!(read(p.to_str().unwrap(), &KEY, "big").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn inner_path_with_spaces() {
        let p = tmp_path("spaces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"Q", "my file.txt").unwrap();
        let bytes = fs::read(&p).unwrap();
        assert!(
            bytes.windows(12).any(|w| w == b"my file.txt "),
            "got: {:?}",
            String::from_utf8_lossy(&bytes[..40])
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn root_offset_points_to_header() {
        // After one append, root entry offset (second token of entry) should
        // point at the local header line for that file.
        let p = tmp_path("offset");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
        let bytes = fs::read(&p).unwrap();
        // find entry line "foo <offset>" in rdd region (текст, plaintext)
        let entry = find_entry_offset(&bytes, "foo").unwrap();
        assert!(
            bytes[entry..].starts_with(b"foo 43"),
            "off={}, bytes={:?}",
            entry,
            String::from_utf8_lossy(&bytes[entry..entry + 6])
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_to_empty_file_errors() {
        let p = tmp_path("emptyfile");
        fs::write(&p, b"").unwrap();
        let res = append(p.to_str().unwrap(), &KEY, b"X", "f");
        assert!(res.is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_to_garbage_errors() {
        let p = tmp_path("garbage");
        fs::write(&p, b"no newline here").unwrap();
        let res = append(p.to_str().unwrap(), &KEY, b"X", "f");
        assert!(res.is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn second_append_preserves_first_data() {
        let p = tmp_path("preserve");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"FIRST", "a").unwrap();
        append(p.to_str().unwrap(), &KEY, b"SECOND", "b").unwrap();
        // plaintext на диске не лежит; обе записи читаются назад
        let bytes = fs::read(&p).unwrap();
        assert!(!bytes.windows(5).any(|w| w == b"FIRST"), "plaintext leaked");
        assert_eq!(read(p.to_str().unwrap(), &KEY, "a").unwrap(), b"FIRST");
        assert_eq!(read(p.to_str().unwrap(), &KEY, "b").unwrap(), b"SECOND");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn eob_last_line_parseable() {
        let p = tmp_path("eob");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
        append(p.to_str().unwrap(), &KEY, b"XY", "bar").unwrap();
        let bytes = fs::read(&p).unwrap();
        let last_nl = bytes[..bytes.len() - 1]
            .iter()
            .rposition(|&b| b == b'\n')
            .unwrap();
        let last = String::from_utf8_lossy(&bytes[last_nl + 1..]);
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
            append(p.to_str().unwrap(), &KEY, &data, &path).unwrap();
        }
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r.len(), 50);
        for i in 0..50 {
            assert!(r.contains(&format!("f{}", i)), "missing f{}", i);
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
        assert!(append(p.to_str().unwrap(), &KEY, b"x", "y").is_ok());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn root_desc_contains_all_entries() {
        let p = tmp_path("rootall");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"a", "one").unwrap();
        append(p.to_str().unwrap(), &KEY, b"b", "two").unwrap();
        append(p.to_str().unwrap(), &KEY, b"c", "three").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r, vec!["one", "two", "three"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn no_double_blank_lines() {
        let p = tmp_path("noblank");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"a", "one").unwrap();
        append(p.to_str().unwrap(), &KEY, b"b", "two").unwrap();
        // случайные байты шифротекстов могут дать \n\n\n — проверяем только
        // текстовые (meta) регионы: маркер+заголовки+rdd+eob
        let bytes = fs::read(&p).unwrap();
        let ct_ranges = ct_payload_ranges(&bytes);
        for (start, end) in ct_ranges {
            let filtered: Vec<u8> = [&bytes[..start], &bytes[end..]].concat();
            assert!(
                !filtered.windows(3).any(|w| w == b"\n\n\n"),
                "too many blanks in meta region"
            );
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn file_size_grows() {
        let p = tmp_path("grow");
        create(p.to_str().unwrap()).unwrap();
        let before = fs::metadata(&p).unwrap().len();
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
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
        append(p.to_str().unwrap(), &KEY, b"a", "one").unwrap();
        append(p.to_str().unwrap(), &KEY, b"b", "two").unwrap();
        append(p.to_str().unwrap(), &KEY, b"c", "three").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert_eq!(r, vec!["one", "two", "three"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_roundtrip_single() {
        let p = tmp_path("readsingle");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"hello world", "foo").unwrap();
        let data = read(p.to_str().unwrap(), &KEY, "foo").unwrap();
        assert_eq!(data, b"hello world");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_roundtrip_empty() {
        let p = tmp_path("readempty");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"", "empty").unwrap();
        let data = read(p.to_str().unwrap(), &KEY, "empty").unwrap();
        assert!(data.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_multiple_files() {
        let p = tmp_path("readmulti");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"AAA", "a").unwrap();
        append(p.to_str().unwrap(), &KEY, b"BBB", "b").unwrap();
        append(p.to_str().unwrap(), &KEY, b"CCC", "c").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "a").unwrap(), b"AAA");
        assert_eq!(read(p.to_str().unwrap(), &KEY, "b").unwrap(), b"BBB");
        assert_eq!(read(p.to_str().unwrap(), &KEY, "c").unwrap(), b"CCC");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_with_newlines() {
        let p = tmp_path("readnl");
        create(p.to_str().unwrap()).unwrap();
        let data = b"x\ny\nz";
        append(p.to_str().unwrap(), &KEY, data, "nl").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "nl").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_large() {
        let p = tmp_path("readlarge");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![b'q'; 50_000];
        append(p.to_str().unwrap(), &KEY, &data, "big").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "big").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_missing_path_errors() {
        let p = tmp_path("readmissing");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"x", "a").unwrap();
        assert!(read(p.to_str().unwrap(), &KEY, "nope").is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_path_with_spaces() {
        let p = tmp_path("readspaces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"SP", "my file.txt").unwrap();
        assert_eq!(
            read(p.to_str().unwrap(), &KEY, "my file.txt").unwrap(),
            b"SP"
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn get_rdd_with_spaces() {
        let p = tmp_path("rddspaces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"SP", "my file.txt").unwrap();
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
                append(p.to_str().unwrap(), &KEY, &data, &name).unwrap();
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
        append(p.to_str().unwrap(), &KEY, b"FIRST", "dup").unwrap();
        append(p.to_str().unwrap(), &KEY, b"SECOND", "dup").unwrap();
        // read should find first occurrence
        let d = read(p.to_str().unwrap(), &KEY, "dup").unwrap();
        assert_eq!(d, b"FIRST");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn empty_inner_path() {
        let p = tmp_path("emptyname");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"X", "").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert!(r.contains(&String::new()));
        assert_eq!(read(p.to_str().unwrap(), &KEY, "").unwrap(), b"X");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn unicode_inner_path() {
        let p = tmp_path("unicode");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"U", "файл/😀").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "файл/😀").unwrap(), b"U");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn data_with_null_bytes() {
        let p = tmp_path("nulls");
        create(p.to_str().unwrap()).unwrap();
        let data = vec![0u8, 1, 2, 0, 3];
        append(p.to_str().unwrap(), &KEY, &data, "n").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "n").unwrap(), data);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn pathbuf_api() {
        let p = tmp_path("pathbuf");
        create(p.to_str().unwrap()).unwrap();
        let pb = std::path::PathBuf::from(&p);
        append(&pb, &KEY, b"P", "pb").unwrap();
        assert_eq!(read(&pb, &KEY, "pb").unwrap(), b"P");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn many_small_appends() {
        let p = tmp_path("manysmall");
        create(p.to_str().unwrap()).unwrap();
        for i in 0..200 {
            append(p.to_str().unwrap(), &KEY, b"x", &format!("s{}", i)).unwrap();
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
            append(p.to_str().unwrap(), &KEY, b"x", &format!("o{}", i)).unwrap();
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
            append(p.to_str().unwrap(), &KEY, &data, &format!("k{}", i)).unwrap();
        }
        for i in 0..50 {
            assert_eq!(
                read(p.to_str().unwrap(), &KEY, &format!("k{}", i)).unwrap(),
                format!("v{}", i).into_bytes()
            );
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn truncated_file_errors() {
        let p = tmp_path("trunc");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
        // truncate last few bytes
        let bytes = fs::read(&p).unwrap();
        fs::write(&p, &bytes[..bytes.len() - 3]).unwrap();
        assert!(
            read(p.to_str().unwrap(), &KEY, "foo").is_err()
                || get_rdd(p.to_str().unwrap()).is_err()
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn corrupted_eob_errors() {
        let p = tmp_path("corrupteob");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"ABC", "foo").unwrap();
        let mut bytes = fs::read(&p).unwrap();
        // replace last line number with garbage
        let idx = bytes.iter().rposition(|&b| b == b'\n').unwrap();
        bytes.truncate(idx + 1);
        bytes.extend_from_slice(b"NOTANUMBER");
        fs::write(&p, &bytes).unwrap();
        assert!(get_rdd(p.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_different_inner_paths() {
        let p = tmp_path("paths");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"1", "alpha").unwrap();
        append(p.to_str().unwrap(), &KEY, b"2", "beta").unwrap();
        append(p.to_str().unwrap(), &KEY, b"3", "gamma").unwrap();
        let r = get_rdd(p.to_str().unwrap()).unwrap();
        assert!(r.contains(&"alpha".to_string()));
        assert!(r.contains(&"beta".to_string()));
        assert!(r.contains(&"gamma".to_string()));
        let _ = fs::remove_file(&p);
    }

    // ============================================================
    // криптография
    // ============================================================

    #[test]
    fn data_on_disk_is_ciphertext() {
        // открытый текст не должен лежать в файле как есть
        let secret = b"super secret plaintext";
        let p = tmp_path("cta");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, secret, "sec").unwrap();
        let bytes = fs::read(&p).unwrap();
        let plaintext = secret.to_vec();
        assert!(
            !bytes.windows(plaintext.len()).any(|w| w == plaintext),
            "plaintext found on disk"
        );
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn same_data_appended_twice_produces_different_ciphertext() {
        // вероятностный тест: nonce разный => шифротексты разных append'ов различны
        let data = b"same bytes";
        let p = tmp_path("nonces");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, data, "a").unwrap();
        append(p.to_str().unwrap(), &KEY, data, "g").unwrap();

        // вытащим оба шифротекста через rdd-оффсеты (это публичное поведение формата)
        let mut file = File::open(&p).unwrap();
        let (eof, eob) = get_last_line_position_and_value(&mut file).unwrap();
        let eob_offset: u64 = eob.trim().parse().unwrap();
        file.seek(SeekFrom::Start(eob_offset)).unwrap();
        let rdd_len = (eof - eob_offset - 1) as usize;
        let mut rdd = vec![0; rdd_len];
        file.read_exact(&mut rdd).unwrap();

        let mut cts: Vec<Vec<u8>> = Vec::new();
        for line in String::from_utf8_lossy(&rdd).lines() {
            if line.trim().is_empty() {
                continue;
            }
            let (_, off) = line.rsplit_once(' ').unwrap();
            let off: usize = off.parse().unwrap();
            file.seek(SeekFrom::Start(off as u64)).unwrap();
            let mut header = Vec::new();
            loop {
                let mut b = [0u8; 1];
                file.read_exact(&mut b).unwrap();
                if b[0] == b'\n' {
                    break;
                }
                header.push(b[0]);
            }
            let h = String::from_utf8_lossy(&header).into_owned();
            let size: usize = h.rsplit_once(' ').unwrap().1.parse().unwrap();
            let mut ct = vec![0; size];
            file.read_exact(&mut ct).unwrap();
            cts.push(ct);
        }

        assert_eq!(cts.len(), 2);
        assert_eq!(cts[0].len(), cts[1].len());
        assert_ne!(cts[0], cts[1], "same plaintext, same ct => nonce reused");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn wrong_key_returns_error_not_garbage() {
        let p = tmp_path("wrongkey");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"top secret", "s").unwrap();
        let bad_key: [u8; 32] = [0x2b; 32];
        let r = read(p.to_str().unwrap(), &bad_key, "s");
        assert!(r.is_err(), "wrong key must error, got {:?}", r);
        if let Err(e) = r {
            assert!(format!("{e}").contains("wrong key"), "err: {e}");
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn tampered_ciphertext_detected() {
        let p = tmp_path("tamper");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"important payload", "t").unwrap();

        // вычисляем точный оффсёт байта внутри шифротекста (внутри ct, после nonce)
        let mut file = File::open(&p).unwrap();
        let (eof, eob) = get_last_line_position_and_value(&mut file).unwrap();
        let eob_offset: u64 = eob.trim().parse().unwrap();
        file.seek(SeekFrom::Start(eob_offset)).unwrap();
        let rdd_len = (eof - eob_offset - 1) as usize;
        let mut rdd = vec![0; rdd_len];
        file.read_exact(&mut rdd).unwrap();
        let line = String::from_utf8_lossy(&rdd)
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap()
            .to_string();
        let (name, off) = line.rsplit_once(' ').unwrap();
        assert_eq!(name, "t");
        let off: usize = off.parse().unwrap();
        file.seek(SeekFrom::Start(off as u64)).unwrap();
        let mut header = Vec::new();
        loop {
            let mut b = [0u8; 1];
            file.read_exact(&mut b).unwrap();
            if b[0] == b'\n' {
                break;
            }
            header.push(b[0]);
        }
        // NONCE_LEN + 5 — гарантированно внутри ct-региона
        let tamper_off = off + header.len() + 1 + NONCE_LEN + 5;
        drop(file);

        let mut bytes = fs::read(&p).unwrap();
        bytes[tamper_off] ^= 0x01;
        fs::write(&p, &bytes).unwrap();
        assert!(read(p.to_str().unwrap(), &KEY, "t").is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn empty_plaintext_still_encrypted() {
        // пустая запись — не пустая на диске: nonce+tag обязательны
        let p = tmp_path("emptyct");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"", "e").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "e").unwrap(), b"");
        assert!(read(p.to_str().unwrap(), &[0u8; 32], "e").is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn ciphertext_survives_binary_content() {
        // зашифрованный регион — бинарный; ни \n, ни \0 внутри не сломают структуру
        for i in 0..8 {
            let p = tmp_path("binloop");
            create(p.to_str().unwrap()).unwrap();
            let data: Vec<u8> = (0..64).map(|j| ((i * 31 + j * 7) % 256) as u8).collect();
            append(p.to_str().unwrap(), &KEY, &data, "bin").unwrap();
            assert_eq!(read(p.to_str().unwrap(), &KEY, "bin").unwrap(), data);
            assert!(get_rdd(p.to_str().unwrap()).is_ok());
            let _ = fs::remove_file(&p);
        }
    }
}
