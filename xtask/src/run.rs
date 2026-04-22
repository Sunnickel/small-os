use std::process::Command;

use crate::run;

pub fn run_qemu() {
    run(Command::new("qemu-system-x86_64").args([
        "-machine",
        "q35",
        "-m",
        "4G",
        "-serial",
        "stdio",
        "-serial",
        "file:.build/serial.log",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-d",
        "int,cpu_reset,guest_errors",
        "-D",
        "qemu.log",
        "-drive",
        "id=boot,format=raw,if=none,file=.build/boot.img",
        "-device",
        "virtio-blk-pci,drive=boot",
        "-drive",
        "id=disk,format=raw,if=none,file=.build/disk.img",
        "-device",
        "virtio-blk-pci,drive=disk",
        "-no-reboot",
        "-no-shutdown",
    ]));
}

pub fn debug() {
    run(Command::new("qemu-system-x86_64").args([
        "-machine",
        "q35",
        "-m",
        "4G",
        "-serial",
        "mon:stdio",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-drive",
        "file=.build/boot.img,format=raw,if=ide,index=0",
        "-drive",
        "id=disk,format=raw,if=none,file=.build/disk.img",
        "-device",
        "virtio-blk-pci,drive=disk",
        "-no-reboot",
        "-s",
        "-S",
    ]));
}
