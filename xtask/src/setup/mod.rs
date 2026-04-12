use std::process::Command;

use crate::run;

pub fn toolchain() { run(Command::new("bash").arg("scripts/toolchain.sh")); }

pub fn image() {
	println!("==> Building boot image");

	// ── boot.img ─────────────────────────────────────────────────────────────
	run(Command::new("bash")
		.args(["-c", "dd if=/dev/zero of=.build/boot.img bs=1M count=64 status=none"]));

	// FAT32 partition at LBA 2048 — do this FIRST before writing bootloader
	run(Command::new("bash").args(["-c", "mkfs.fat -F 32 -n BOOTFS --offset 2048 .build/boot.img"]));

	// Bootloader chain — written AFTER mkfs.fat so it doesn't get wiped
	run(Command::new("bash").args([
		"-c",
		"dd if=.build/stage1.bin of=.build/boot.img bs=512 seek=0 conv=notrunc status=none",
	]));
	run(Command::new("bash").args([
		"-c",
		"dd if=.build/stage2.bin of=.build/boot.img bs=512 seek=1 conv=notrunc status=none",
	]));
	run(Command::new("bash").args([
		"-c",
		"dd if=.build/stage3.bin of=.build/boot.img bs=512 seek=32 conv=notrunc status=none",
	]));

	// Copy payloads into FAT32 partition
	run(Command::new("bash")
		.args(["-c", "mcopy -i .build/boot.img@@1048576 .build/kernel.elf ::KERNEL.ELF"]));
	run(Command::new("bash")
		.args(["-c", "mcopy -i .build/boot.img@@1048576 .build/installer.elf ::INSTALL.ELF"]));
	run(Command::new("bash")
		.args(["-c", "mcopy -i .build/boot.img@@1048576 .build/stage1.bin ::STAGE1.BIN"]));
	run(Command::new("bash")
		.args(["-c", "mcopy -i .build/boot.img@@1048576 .build/stage2.bin ::STAGE2.BIN"]));
	run(Command::new("bash")
		.args(["-c", "mcopy -i .build/boot.img@@1048576 .build/stage3.bin ::STAGE3.BIN"]));

	// ── disk.img ─────────────────────────────────────────────────────────────
	run(Command::new("bash")
		.args(["-c", "dd if=/dev/zero of=.build/disk.img bs=1M count=512 status=none"]));
}

pub fn clean() { run(Command::new("rm").args(["-rf", ".build"])); }
