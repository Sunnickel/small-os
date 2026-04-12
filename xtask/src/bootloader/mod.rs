use std::process::Command;

use crate::{build_stage3, run};

pub(crate) fn build_bootloader() {
    build_stage1();
    build_stage2();
    build_stage3();
}

fn build_stage1() {
    run(Command::new("nasm").args([
        "-f",
        "bin",
        "bootloader/stage1/main.asm",
        "-o",
        ".build/stage1.bin",
    ]));
}

fn build_stage2() {
    run(Command::new("nasm").args([
        "-f",
        "bin",
        "bootloader/stage2/main.asm",
        "-o",
        ".build/stage2.bin",
    ]));
}

fn build_stage3() { build_stage3::build_stage3(); }
