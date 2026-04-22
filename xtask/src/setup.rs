use std::{fs, process::Command};

use crate::run;

pub fn toolchain() {
    run(Command::new("bash").arg("-c").arg("tr -d '\\r' < scripts/toolchain.sh | bash"));
}

pub fn image() {
    println!("==> Building boot image");

    // ─────────────────────────────────────────────
    // 1. Create empty disk
    // ─────────────────────────────────────────────
    run(Command::new("bash")
        .args(["-c", "dd if=/dev/zero of=.build/boot.img bs=1M count=64 status=none"]));

    // ─────────────────────────────────────────────
    // 2. FAT filesystem
    // ─────────────────────────────────────────────
    run(Command::new("bash").args(["-c", "mkfs.fat -F 32 -n BOOTFS --offset 2048 .build/boot.img"]));

    // ─────────────────────────────────────────────
    // 3. Compute Stage3 size (IMPORTANT FIX)
    // ─────────────────────────────────────────────
    let stage3_size = fs::metadata(".build/stage3.bin").expect("stage3 missing").len();

    let stage3_sectors = ((stage3_size + 511) / 512) as u16;

    // ─────────────────────────────────────────────
    // 4. PATCH stage3_sectors INTO stage1.bin (IMPORTANT: you MUST know offset in
    //    binary)
    // ─────────────────────────────────────────────
    let mut stage1 = fs::read(".build/stage1.bin").unwrap();

    let offset = 0x1A0;

    stage1[offset] = (stage3_sectors & 0xFF) as u8;
    stage1[offset + 1] = (stage3_sectors >> 8) as u8;

    fs::write(".build/stage1.bin", &stage1).unwrap();

    // ─────────────────────────────────────────────
    // 5. Write bootloader chain
    // ─────────────────────────────────────────────
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

    // ─────────────────────────────────────────────
    // 6. Copy payloads into FAT32
    // ─────────────────────────────────────────────
    let fat = ".build/boot.img@@1048576";

    run(Command::new("bash")
        .args(["-c", &format!("mcopy -i {} .build/kernel.elf ::KERNEL.ELF", fat)]));

    run(Command::new("bash")
        .args(["-c", &format!("mcopy -i {} .build/installer.elf ::INSTALL.ELF", fat)]));

    run(Command::new("bash")
        .args(["-c", &format!("mcopy -i {} .build/stage1.bin ::STAGE1.BIN", fat)]));

    run(Command::new("bash")
        .args(["-c", &format!("mcopy -i {} .build/stage2.bin ::STAGE2.BIN", fat)]));

    run(Command::new("bash")
        .args(["-c", &format!("mcopy -i {} .build/stage3.bin ::STAGE3.BIN", fat)]));

    // ─────────────────────────────────────────────
    // 7. Extra disk
    // ─────────────────────────────────────────────
    run(Command::new("bash")
        .args(["-c", "dd if=/dev/zero of=.build/disk.img bs=1M count=512 status=none"]));
}

pub fn clean() { run(Command::new("rm").args(["-rf", ".build"])); }
