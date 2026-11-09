use anyhow::{Result, bail};
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyInit, KeyIvInit, block_padding::Pkcs7};
use cmac::{Cmac, Mac};
use crate::sm4::Sm4;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
};
type Sm4CbcEnc = cbc::Encryptor<Sm4>;
type Sm4CbcDec = cbc::Decryptor<Sm4>;

pub const KEY_LEN: usize = 32;
const BLOCK: usize = 16;
const TAG_LEN: usize = BLOCK;
const OVERHEAD: usize = BLOCK + TAG_LEN;

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
    file.lock()?;
    let (eof, raw_rdd) = read_rdd_region(&mut file)?;

    let mut rdd = decrypt_rdd(key, &raw_rdd)?;
    if !rdd.is_empty() && !rdd.ends_with(b"\n") {
        rdd.push(b'\n');
    }
    rdd.extend_from_slice(format!("{} {}", inner_path, eof).as_bytes());
    let rdd_ciphertext = encrypt(key, &rdd)?;

    // '\n' guaranteed
    file.seek(SeekFrom::Start(eof))?;
    file.write_all(format!("{} {}\n", inner_path, ciphertext.len()).as_bytes())?;
    file.write_all(&ciphertext)?;
    file.write_all(b"\n")?; // here it appears
    let eob_new = file.stream_position()?;
    file.write_all(&rdd_ciphertext)?;
    file.write_all(b"\n")?;
    file.write_all(format!(" {}", eob_new).as_bytes())?;
    Ok(())
}

pub fn get_rdd<P: AsRef<std::path::Path>>(path: P, key: &[u8; KEY_LEN]) -> Result<Vec<String>> {
    let path = path.as_ref();
    let mut file = OpenOptions::new().read(true).open(path)?;
    file.lock_shared()?;
    let (_, raw_rdd) = read_rdd_region(&mut file)?;
    let rdd = decrypt_rdd(key, &raw_rdd)?;
    Ok(parse_rdd(&rdd)?.into_iter().map(|(name, _)| name).collect())
}

pub fn read<P: AsRef<std::path::Path>>(
    path: P,
    key: &[u8; KEY_LEN],
    inner_path: &str,
) -> Result<Vec<u8>> {
    let path = path.as_ref();
    let mut file = OpenOptions::new().read(true).open(path)?;
    file.lock_shared()?;
    let (_, raw_rdd) = read_rdd_region(&mut file)?;
    let rdd = decrypt_rdd(key, &raw_rdd)?;
    let offset = parse_rdd(&rdd)?
        .into_iter()
        .find(|(name, _)| name == inner_path)
        .map(|(_, offset)| offset)
        .ok_or_else(|| anyhow::anyhow!("inner_path not found in rdd"))?;

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
    if size < OVERHEAD + BLOCK {
        bail!("payload too small: {}", size);
    }
    let mut payload = vec![0; size];
    file.read_exact(&mut payload)?;
    decrypt(key, &payload)
}

fn read_rdd_region(file: &mut File) -> Result<(u64, Vec<u8>)> {
    let (eof, eob) = get_last_line_position_and_value(file)?;
    let eob_offset: u64 = eob.parse().map_err(|_| anyhow::anyhow!("invalid eob"))?;
    let rdd_len = eof.saturating_sub(eob_offset).saturating_sub(1);
    file.seek(SeekFrom::Start(eob_offset))?;
    let mut rdd = vec![0; rdd_len as usize];
    if rdd_len > 0 {
        file.read_exact(&mut rdd)?;
    }
    Ok((eof, rdd))
}

fn decrypt_rdd(key: &[u8; KEY_LEN], raw_rdd: &[u8]) -> Result<Vec<u8>> {
    if raw_rdd.is_empty() {
        Ok(Vec::new())
    } else {
        decrypt(key, raw_rdd)
    }
}

fn parse_rdd(rdd: &[u8]) -> Result<Vec<(String, u64)>> {
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(rdd).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (name, offset) = line
            .rsplit_once(' ')
            .ok_or_else(|| anyhow::anyhow!("bad rdd entry"))?;
        let offset = offset.parse().map_err(|_| anyhow::anyhow!("bad offset"))?;
        out.push((name.to_string(), offset));
    }
    Ok(out)
}

fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
    let (enc_key, mac_key) = split_key(key);
    let mut iv = [0u8; BLOCK];
    getrandom::fill(&mut iv)?;

    // PKCS#7 填充总会额外添加至少一个分组。
    let padded_len = (plaintext.len() / BLOCK + 1) * BLOCK;
    let mut buf = vec![0u8; padded_len];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ct_len = Sm4CbcEnc::new(&enc_key.into(), &iv.into())
        .encrypt_padded::<Pkcs7>(&mut buf, plaintext.len())
        .map_err(|_| anyhow::anyhow!("encrypt failed: buffer too small"))?
        .len();
    buf.truncate(ct_len);

    // 布局：iv || 密文 || cmac(iv || 密文)
    let mut out = Vec::with_capacity(BLOCK + buf.len() + TAG_LEN);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&buf);
    let tag = cmac_sm4(&mac_key, &out)?;
    out.extend_from_slice(&tag);
    Ok(out)
}

fn decrypt(key: &[u8; KEY_LEN], payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() < OVERHEAD + BLOCK {
        bail!("payload too small: {}", payload.len());
    }
    let (iv, rest) = payload.split_at(BLOCK);
    let (ct, tag) = rest.split_at(rest.len() - TAG_LEN);
    if ct.is_empty() || ct.len() % BLOCK != 0 {
        bail!("invalid ciphertext length: {}", ct.len());
    }

    let (enc_key, mac_key) = split_key(key);
    let mut mac = Cmac::<Sm4>::new_from_slice(&mac_key)
        .map_err(|_| anyhow::anyhow!("wrong key or corrupted data"))?;
    mac.update(&payload[..payload.len() - TAG_LEN]);
    mac.verify_slice(tag)
        .map_err(|_| anyhow::anyhow!("wrong key or corrupted data"))?;

    let iv: [u8; BLOCK] = iv.try_into().expect("iv len checked");
    let mut buf = ct.to_vec();
    let pt_len = Sm4CbcDec::new(&enc_key.into(), &iv.into())
        .decrypt_padded::<Pkcs7>(&mut buf)
        .map_err(|_| anyhow::anyhow!("wrong key or corrupted data"))?
        .len();
    buf.truncate(pt_len);
    Ok(buf)
}

fn split_key(key: &[u8; KEY_LEN]) -> ([u8; BLOCK], [u8; BLOCK]) {
    let mut enc = [0u8; BLOCK];
    let mut mac = [0u8; BLOCK];
    enc.copy_from_slice(&key[..BLOCK]);
    mac.copy_from_slice(&key[BLOCK..]);
    (enc, mac)
}

fn cmac_sm4(key: &[u8; BLOCK], data: &[u8]) -> Result<[u8; TAG_LEN]> {
    let mut mac =
        Cmac::<Sm4>::new_from_slice(key).map_err(|_| anyhow::anyhow!("bad CMAC key length"))?;
    mac.update(data);
    let tag: [u8; TAG_LEN] = mac.finalize().into_bytes().into();
    Ok(tag)
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

    fn rdd_range_from_bytes(bytes: &[u8]) -> (usize, usize) {
        let idx_last_nl = bytes.iter().rposition(|&b| b == b'\n').unwrap();
        let eob: usize = String::from_utf8_lossy(&bytes[idx_last_nl + 1..])
            .trim()
            .parse()
            .unwrap();
        (eob, idx_last_nl)
    }

    fn parse_rdd_from_bytes(bytes: &[u8]) -> Vec<(String, usize)> {
        let (eob, idx_last_nl) = rdd_range_from_bytes(bytes);
        let rdd = decrypt_rdd(&KEY, &bytes[eob..idx_last_nl]).unwrap();
        parse_rdd(&rdd)
            .unwrap()
            .into_iter()
            .map(|(name, off)| (name, off as usize))
            .collect()
    }

    fn find_entry_offset(bytes: &[u8], name: &str) -> Option<usize> {
        parse_rdd_from_bytes(bytes)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| o)
    }

    fn encrypted_ranges(bytes: &[u8]) -> Vec<(usize, usize)> {
        let entries = parse_rdd_from_bytes(bytes);
        let mut out = Vec::new();
        for (i, (_, off)) in entries.iter().enumerate() {
            let nl = bytes[*off..].iter().position(|&b| b == b'\n').unwrap();
            let header = String::from_utf8_lossy(&bytes[*off..*off + nl]).into_owned();
            let size: usize = header.rsplit_once(' ').unwrap().1.parse().unwrap();
            let ct_start = *off + nl + 1;
            let ct_end = ct_start + size;
            out.push((ct_start, ct_end));
            if let Some((_, next_off)) = entries.get(i + 1) {
                // 记录之间遗留的旧 RDD 密文
                out.push((ct_end, *next_off));
            }
        }
        out.push(rdd_range_from_bytes(bytes));
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
            bytes.starts_with(b"\nfoo 48\n"),
            "got: {:?}",
            String::from_utf8_lossy(&bytes[..40])
        );
        let header_end = b"\nfoo 48\n".len();
        let ct_region = &bytes[header_end..header_end + 48];
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
        // 空明文 => 16（iv）+ 16（填充）+ 16（tag）= 48 字节
        assert!(
            bytes.starts_with(b"\nempty 48\n"),
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
            bytes[entry..].starts_with(b"foo 48"),
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
        // 明文不落盘；两条记录都会被回读。
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
        assert_eq!(r, vec!["one", "two", "three"]);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn no_double_blank_lines() {
        let p = tmp_path("noblank");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"a", "one").unwrap();
        append(p.to_str().unwrap(), &KEY, b"b", "two").unwrap();
        // 密文的随机字节可能产生 \n\n\n —— 因此排除数据密文和 RDD 密文，
        // 只检查纯文本的元数据区域。
        let bytes = fs::read(&p).unwrap();
        let mut ranges = encrypted_ranges(&bytes);
        ranges.sort_unstable();

        let mut pos = 0;
        for (start, end) in ranges {
            if start > pos {
                assert!(
                    !bytes[pos..start].windows(3).any(|w| w == b"\n\n\n"),
                    "too many blanks in meta region"
                );
            }
            pos = pos.max(end);
        }
        assert!(
            !bytes[pos..].windows(3).any(|w| w == b"\n\n\n"),
            "too many blanks in meta region"
        );
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
        assert!(r.contains(&String::new()));
        assert_eq!(read(p.to_str().unwrap(), &KEY, "").unwrap(), b"X");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn unicode_inner_path() {
        let p = tmp_path("unicode");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"U", "файл/🙏").unwrap();
        assert_eq!(read(p.to_str().unwrap(), &KEY, "файл/🙏").unwrap(), b"U");
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
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
                || get_rdd(p.to_str().unwrap(), &KEY).is_err()
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
        assert!(get_rdd(p.to_str().unwrap(), &KEY).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn append_different_inner_paths() {
        let p = tmp_path("paths");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"1", "alpha").unwrap();
        append(p.to_str().unwrap(), &KEY, b"2", "beta").unwrap();
        append(p.to_str().unwrap(), &KEY, b"3", "gamma").unwrap();
        let r = get_rdd(p.to_str().unwrap(), &KEY).unwrap();
        assert!(r.contains(&"alpha".to_string()));
        assert!(r.contains(&"beta".to_string()));
        assert!(r.contains(&"gamma".to_string()));
        let _ = fs::remove_file(&p);
    }

    // ============================================================
    // 密码学
    // ============================================================

    #[test]
    fn data_on_disk_is_ciphertext() {
        // 明文不应原样存放在文件中
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
    fn rdd_is_encrypted_and_requires_key() {
        let p = tmp_path("rdd_ct");
        create(p.to_str().unwrap()).unwrap();
        append(p.to_str().unwrap(), &KEY, b"x", "secret").unwrap();

        let bytes = fs::read(&p).unwrap();
        let (eob, idx_last_nl) = rdd_range_from_bytes(&bytes);
        let raw_rdd = &bytes[eob..idx_last_nl];
        let plain = decrypt(&KEY, raw_rdd).unwrap();
        assert!(plain.starts_with(b"secret "));
        assert_ne!(raw_rdd, plain.as_slice());

        let bad_key: [u8; 32] = [0x2b; 32];
        assert!(decrypt(&bad_key, raw_rdd).is_err());
        assert!(get_rdd(p.to_str().unwrap(), &bad_key).is_err());
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
        let entries = parse_rdd_from_bytes(&fs::read(&p).unwrap());
        let mut file = File::open(&p).unwrap();

        let mut cts: Vec<Vec<u8>> = Vec::new();
        for (_, off) in entries {
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

        // 计算字节在密文中的精确偏移量（在 ct 内，位于 iv 之后）。
        let bytes = fs::read(&p).unwrap();
        let (name, off) = parse_rdd_from_bytes(&bytes)
            .into_iter()
            .find(|(name, _)| name == "t")
            .unwrap();
        assert_eq!(name, "t");
        let mut file = File::open(&p).unwrap();
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
        // BLOCK + 5 —— 确保位于密文区域内
        let tamper_off = off + header.len() + 1 + BLOCK + 5;
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
    fn cmac_sm4_known_answer() {
        // OpenSSL 3.6: `openssl mac -macopt cipher:SM4-CBC -macopt hexkey:<key> CMAC`
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let cases: [(&[u8], [u8; TAG_LEN]); 4] = [
            (
                &[],
                [
                    0x39, 0x9a, 0x9c, 0x93, 0x09, 0x64, 0xa3, 0xd4, 0xe3, 0x8c, 0x59, 0xda, 0x47,
                    0xf0, 0xb3, 0x09,
                ],
            ),
            (
                &[
                    0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73,
                    0x93, 0x17, 0x2a,
                ],
                [
                    0x4e, 0x4c, 0x2a, 0x44, 0x17, 0xe5, 0x67, 0xfe, 0xf0, 0x81, 0xe0, 0xfa, 0xb5,
                    0x5a, 0x57, 0x62,
                ],
            ),
            (
                &[
                    0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73,
                    0x93, 0x17, 0x2a, 0xae, 0x2d, 0x8a, 0x57, 0x1e, 0x03, 0xac, 0x9c, 0x9e, 0xb7,
                    0x6f, 0xac, 0x45, 0xaf, 0x8e, 0x51,
                ],
                [
                    0x93, 0xe3, 0x5c, 0x4a, 0xf3, 0xe2, 0xb4, 0x42, 0x3f, 0x07, 0xbe, 0x9b, 0x87,
                    0xaa, 0x6b, 0x3f,
                ],
            ),
            (
                &[
                    0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73,
                    0x93, 0x17, 0x2a, 0xae, 0x2d, 0x8a, 0x57, 0x1e, 0x03, 0xac, 0x9c, 0x9e, 0xb7,
                    0x6f, 0xac, 0x45, 0xaf, 0x8e, 0x51, 0xda, 0xc6, 0xc2, 0xf2, 0xb4, 0xe1, 0xb3,
                    0xd4, 0xd1, 0xf2, 0xe3, 0xa4, 0xb5, 0xc6, 0xd7, 0xe8,
                ],
                [
                    0x9e, 0x44, 0x3c, 0x08, 0x4d, 0xfd, 0x6e, 0x97, 0x61, 0x2b, 0x51, 0xe7, 0xc9,
                    0xe7, 0x14, 0x93,
                ],
            ),
        ];
        for (data, expected) in cases {
            assert_eq!(cmac_sm4(&key, data).unwrap(), expected);
        }
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
            assert!(get_rdd(p.to_str().unwrap(), &KEY).is_ok());
            let _ = fs::remove_file(&p);
        }
    }
}
