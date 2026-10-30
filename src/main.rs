use anyhow::{Result, bail};
use chrono::{Datelike, Local, Timelike};
use std::{
    collections::HashMap, env, fs, io::{self, Write}, path::Path, process::Command,
};

mod protocol;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let mut params: HashMap<&str, &str> = HashMap::new();
    let mut path: Option<&Path> = None;

    // parse args
    if args.len() > 1 {
        let mut next_param: Option<&str> = None;
        for d in &args[1..args.len()] {
            if let Some(_) = next_param {
                params.insert(next_param.unwrap(), d);
                continue;
            }
            if d.contains("--") {
                next_param = Some(d);
            } else {
                match path {
                    Some(_) => bail!("some arguments shit"),
                    None => path = Some(Path::new(d)),
                }
            }
        }
    } else {
        bail!("no path provided");
    }

    // get rdd
    let mut rdd: Vec<String> = Vec::new();
    match path.unwrap().try_exists() {
        Ok(true) => {
            let mut rddt = protocol::get_rdd(path.unwrap()).unwrap();
            rdd.append(&mut rddt);
        }
        Ok(false) => {
            protocol::create(&path.unwrap()).unwrap();
        }
        Err(e) => bail!("{e}, file isn't reachable"),
    }

    // now if params
    // --list: true - print rdd
    // --append: true - open vim and append
    // --upload: <file> - upload file
    // --read: <internal_file> - print file
    if params.get("--list") == Some(&"true") {
        println!("{:?}", rdd);
    }
    if params.get("--append") == Some(&"true") {
        let shmpath_raw = format!("/dev/shm/secure_tempfile_{}", std::process::id());
        let shmpath = Path::new(&shmpath_raw);
        fs::write(shmpath, &format!("{}", Local::now()))?;
        let status = Command::new("nvim")
            .arg("-n")
            .arg("-i")
            .arg("NONE")
            .arg(shmpath)
            .status()?;

        if status.success() {
            let edited = fs::read(shmpath)?;
            let now = Local::now();
            let today = format!("{}\\{:02}\\{:02}/", now.year(), now.month(), now.day());
            let rn = format!(
                "{:02}:{:02}:{:02}.yad",
                now.hour(),
                now.minute(),
                now.second()
            );
            if protocol::read(path.unwrap(), &format!("{today}{rn}")).is_ok() {
                bail!("file with same date and time exists");
            }
            protocol::append(path.unwrap(), &edited, &format!("{today}{rn}"))?; // format is not particular
        }
        if shmpath.exists() {
            let file_size = fs::metadata(shmpath)?.len();
            let mut file = fs::OpenOptions::new().write(true).open(shmpath)?;
            let zero_buffer = vec![0u8; file_size as usize];
            file.write_all(&zero_buffer)?;
            file.flush()?;
            fs::remove_file(shmpath)?;
        }
    }
    if let Some(x) = params.get("--upload") {
        if let Ok(_) = protocol::read(path.unwrap(), x) {
            bail!("same name");
        }
        let file = fs::read(x)?;
        if file.is_empty() {
            bail!("no files with null lengh");
        }
        protocol::append(path.unwrap(), &file, x)?;
    }
    if let Some(x) = params.get("--read") {
        io::stdout().write_all(&protocol::read(path.unwrap(), x)?)?;
    }

    Ok(())
}
