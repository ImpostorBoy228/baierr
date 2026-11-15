use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let affap = manifest_dir.join("affap");

    println!("cargo:rerun-if-changed={}", affap.join("main.c").display());
    println!("cargo:rerun-if-changed={}", affap.join("Makefile").display());

    let status = Command::new("make")
        .arg("-C")
        .arg(&affap)
        .status()
        .expect("failed to invoke make");
    if !status.success() {
        panic!("make failed in affap (exit status {})", status);
    }
}
