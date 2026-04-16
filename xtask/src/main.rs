use std::{fs, process::Command};
use std::process::Stdio;
use clap::{Parser, Subcommand};

use crate::{
    bootloader::build_bootloader,
    build::{build_installer, build_kernel},
    run::{debug, run_qemu},
    setup::{clean, image, toolchain},
};

mod bootloader;
mod build;
mod build_stage3;
mod cache;
mod hash;
mod run;
mod setup;

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
        Cmd::Build => build(),
        Cmd::Run => {
            build();
            run_qemu()
        }
        Cmd::Debug => {
            build();
            debug()
        }
        Cmd::Clean => clean(),
    }
}

fn build() {
    if !fs::exists("./.build").unwrap() {
        fs::create_dir("./.build").unwrap();
    }

    build_bootloader();
    build_kernel();
    build_installer();
    image();
}
