use anyhow::{Result, bail};
use chrono::{Datelike, Local, Timelike};
use clap::{Args, Parser};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

mod protocol;

#[derive(Parser)]
#[command(name = "baierr", version, about = "secrets and shit")]
struct Cli {
    /// .b path
    archive: PathBuf,

    #[command(flatten)]
    action: Action,
}

#[derive(Args)]
#[group(required = false, multiple = false)]
struct Action {
    /// list files
    #[arg(short = 'l', long)]
    list: bool,

    /// add one record using nvim
    #[arg(short = 'a', long)]
    append: bool,

    /// upload to archive
    #[arg(short = 'u', long, value_names = ["FILE", "NAME"], num_args = 2)]
    upload: Option<Vec<PathBuf>>,

    /// print one record
    #[arg(short = 'r', long, value_name = "NAME")]
    read: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let archive = cli.archive;
    match archive.try_exists() {
        Ok(true) => {}
        Ok(false) => protocol::create(&archive)?,
        Err(e) => bail!("{e}, file isn't reachable"),
    }

    let a = &cli.action;
    if a.append {
        cmd_append(&archive)?;
    } else if let Some([file, name]) = a.upload.as_deref() {
        // num_args = 2 гарантирует [file, name]
        cmd_upload(&archive, file, &name.to_string_lossy())?;
    } else if let Some(name) = &a.read {
        io::stdout().write_all(&protocol::read(&archive, name)?)?;
    } else {
        // включая явный --list и «голый» вызов
        let rdd = protocol::get_rdd(&archive)?;
        println!("{:?}", rdd);
    }

    Ok(())
}

fn cmd_append(archive: &Path) -> Result<()> {
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
        if protocol::read(archive, &format!("{today}{rn}")).is_ok() {
            bail!("file with same date and time exists");
        }
        protocol::append(archive, &edited, &format!("{today}{rn}"))?; // format is not particular
    }
    if shmpath.exists() {
        let file_size = fs::metadata(shmpath)?.len();
        let mut file = fs::OpenOptions::new().write(true).open(shmpath)?;
        let zero_buffer = vec![0u8; file_size as usize];
        file.write_all(&zero_buffer)?;
        file.flush()?;
        fs::remove_file(shmpath)?;
    }

    Ok(())
}

fn cmd_upload(archive: &Path, file: &Path, name: &str) -> Result<()> {
    if protocol::read(archive, name).is_ok() {
        bail!("same name");
    }
    let data = fs::read(file)?;
    if data.is_empty() {
        bail!("no files with null lengh");
    }
    protocol::append(archive, &data, name)?;

    Ok(())
}
