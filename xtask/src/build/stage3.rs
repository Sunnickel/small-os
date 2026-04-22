use std::process::Command;

use crate::{
    cache::BuildCache,
    consts::{BUILD_DIR, STAGE3_SRC},
    discover::{find_asm_files, find_c_files},
    hash::hash_file,
    run,
};

const KEY: &str = "stage3_build";
const OUT: &str = ".build/stage3";

pub fn build(cache: &mut BuildCache) {
    let c_files = find_c_files(STAGE3_SRC);
    let asm_files = find_asm_files(STAGE3_SRC);

    let combined_hash = {
        let mut all = String::new();
        for f in c_files.iter().chain(asm_files.iter()) {
            all.push_str(&hash_file(f));
        }
        all
    };

    if !cache.is_dirty(KEY, &combined_hash) {
        return;
    }

    println!("building stage3...");

    std::fs::create_dir_all(OUT).unwrap();

    // Compile .c files
    for src in &c_files {
        let obj = format!("{}/{}.o", OUT, src.replace('/', "_"));
        run(Command::new("x86_64-elf-gcc").args([
            "-ffreestanding",
            "-nostdlib",
            "-m64",
            "-O2",
            "-c",
            src,
            "-o",
            &obj,
        ]));
    }

    // Assemble .asm files
    for src in &asm_files {
        let obj = format!("{}/{}.o", OUT, src.replace('/', "_"));
        run(Command::new("nasm").args(["-f", "elf64", src, "-o", &obj]));
    }

    // Collect all .o files
    let objs: Vec<String> = std::fs::read_dir(OUT)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_string_lossy().to_string())
        .filter(|p| p.ends_with(".o"))
        .collect();

    // Link
    run(Command::new("x86_64-elf-ld").args(&objs).args([
        "-T",
        &"bootloader/stage3/linker.ld".to_string(),
        "-nostdlib",
        "--build-id=none",
        "-o",
        &format!("{}/stage3.elf", BUILD_DIR),
    ]));

    // Strip to flat binary
    run(Command::new("x86_64-elf-objcopy").args([
        "-O",
        "binary",
        &format!("{}/stage3.elf", BUILD_DIR),
        &format!("{}/stage3.bin", BUILD_DIR),
    ]));

    cache.update(KEY, combined_hash);
}
