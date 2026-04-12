use std::{fs, process::Command};

use crate::run;

pub fn build_installer() {
    run(Command::new("cargo").args([
        "build",
        "--package",
        "installer",
        "--release",
        "--target",
        "x86_64-unknown-none",
    ]));

    fs::create_dir_all(".build").unwrap();

    fs::copy("target/x86_64-unknown-none/release/installer", ".build/installer.elf").unwrap();
}

pub fn build_kernel() {
    run(Command::new("cargo").args([
        "build",
        "--package",
        "kernel",
        "--release",
        "--target",
        "x86_64-unknown-none",
    ]));

    fs::create_dir_all(".build").unwrap();

    fs::copy("target/x86_64-unknown-none/release/kernel", ".build/kernel.elf").unwrap();
}
