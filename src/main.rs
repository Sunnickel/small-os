mod disk;

use std::{env, path::Path, process::Command};

use ovmf_prebuilt::{Arch, FileType, Prebuilt, Source};

const DISK_PATH: &str = "target/disk.img";

fn main() {
    let uefi_path = env!("UEFI_PATH");
    let bios_path = env!("BIOS_PATH");

    let args: Vec<String> = env::args().collect();
    let prog = &args[0];

    let uefi = match args.get(1).map(|s| s.to_lowercase()) {
        Some(ref s) if s == "uefi" => true,
        Some(ref s) if s == "bios" => false,
        Some(ref s) if s == "-h" || s == "--help" => {
            println!("Usage: {prog} [uefi|bios] [virtio|ahci]");
            println!("  uefi   - boot using OVMF (UEFI)");
            println!("  bios   - boot using legacy BIOS");
            println!("  virtio - attach disk as VirtIO block device (default)");
            println!("  ahci   - attach disk as AHCI/SATA device");
            std::process::exit(0);
        }
        _ => {
            eprintln!("Usage: {prog} [uefi|bios] [virtio|ahci]");
            std::process::exit(1);
        }
    };

    let use_ahci = matches!(args.get(2).map(|s| s.to_lowercase()).as_deref(), Some("ahci"));

    println!("Booting {}...", if uefi { "UEFI" } else { "BIOS" });
    println!("Using {}...", if use_ahci { "AHCI" } else { "VirtIO" });

    if !Path::new(DISK_PATH).exists() {
        eprintln!("Creating new GPT/NTFS disk image at {DISK_PATH}...");
        disk::create_ntfs_disk(DISK_PATH).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });
    }

    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.arg("-machine").arg("q35");
    cmd.arg("-serial").arg("mon:stdio");
    cmd.arg("-device").arg("isa-debug-exit,iobase=0xf4,iosize=0x04");

    if use_ahci {
        cmd.arg("-device").arg("ich9-ahci,id=ahci");
        cmd.arg("-drive")
            .arg(format!("id=disk0,file={DISK_PATH},format=raw,if=none,cache=writethrough"));
        cmd.arg("-device").arg("ide-hd,drive=disk0,bus=ahci.0");
    } else {
        cmd.arg("-drive")
            .arg(format!("id=disk0,file={DISK_PATH},format=raw,if=none,cache=writethrough"));
        cmd.arg("-device").arg("virtio-blk-pci,drive=disk0,disable-legacy=on,disable-modern=off");
    }

    if uefi {
        let prebuilt =
            Prebuilt::fetch(Source::LATEST, "target/ovmf").expect("failed to update prebuilt");
        let code = prebuilt.get_file(Arch::X64, FileType::Code);
        let vars = prebuilt.get_file(Arch::X64, FileType::Vars);
        cmd.arg("-drive").arg(format!("format=raw,file={uefi_path}"));
        cmd.arg("-drive")
            .arg(format!("if=pflash,format=raw,unit=0,file={},readonly=on", code.display()));
        cmd.arg("-drive")
            .arg(format!("if=pflash,format=raw,unit=1,file={},snapshot=on", vars.display()));
    } else {
        cmd.arg("-drive").arg(format!("format=raw,file={bios_path}"));
    }

    let mut child = cmd.spawn().expect("failed to start qemu-system-x86_64");
    let status = child.wait().expect("failed to wait on qemu");
    match status.code().unwrap_or(1) {
        0x10 => std::process::exit(0),
        0x11 => std::process::exit(1),
        _ => std::process::exit(2),
    }
}
