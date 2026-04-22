use std::{
    fs,
    process::{Command, Stdio},
};

use clap::{Parser, Subcommand};

use crate::{
    build::{build_bootloader, build_installer, build_kernel},
    consts::BUILD_DIR,
    run::{debug, run_qemu},
    setup::{clean, image, toolchain},
};

mod build;
mod cache;
mod discover;
mod format;
mod hash;
mod run;
mod setup;

mod consts {
    pub(crate) static BUILD_DIR: &str = ".build";

    pub(crate) static STAGE1_SRC: &str = "bootloader/stage1";
    pub(crate) static STAGE2_SRC: &str = "bootloader/stage2";
    pub(crate) static STAGE3_SRC: &str = "bootloader/stage3/src";

    pub const WORKSPACE_CRATES: &[&str] = &[
        "installer",
        "kernel",
        "lib/driver",
        "lib/macros",
        "lib/hal",
        "lib/boot",
        "lib/device",
        "lib/vfs",
        "lib/sync",
        "lib/bus",
    ];
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Toolchain,
    Build,
    Run,
    Debug,
    Clean,
    Fmt,
    Check,
}

pub fn run(cmd: &mut Command) {
    let output = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("failed to execute command");

    if !output.status.success() {
        panic!("command failed: {:?}", output.status);
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Toolchain => toolchain(),

        Cmd::Fmt => fmt(),

        Cmd::Build => build(),

        Cmd::Check => {
            fmt();
            build();
        }

        Cmd::Run => {
            build();
            run_qemu();
        }

        Cmd::Debug => {
            build();
            debug();
        }

        Cmd::Clean => clean(),
    }
}

fn build() {
    fs::create_dir_all(BUILD_DIR).unwrap();

    build_bootloader();
    build_kernel();
    build_installer();

    image();
}

fn fmt() { format::format_all(); }
