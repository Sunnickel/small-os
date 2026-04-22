mod stage1;
mod stage2;
mod stage3;

use std::{fs, process::Command};

use crate::{cache::BuildCache, run};

pub fn build_bootloader() {
    let mut cache = BuildCache::load();

    stage1::build(&mut cache);
    stage2::build(&mut cache);
    stage3::build(&mut cache);
}

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
