use std::process::Command;

use rayon::prelude::*;

use crate::{cache::BuildCache, hash::hash_file};

const SRC: &str = "bootloader/stage3/src";
const OUT: &str = ".build/stage3";

fn build_c(cache: &mut BuildCache, files: &[&str]) {
    std::fs::create_dir_all(".build/stage3").unwrap();

    files.par_iter().for_each(|file| {
        let src = format!("{}/{}", SRC, file);
        let hash = hash_file(&src);

        let obj = format!("{}/{}.o", OUT, file.strip_suffix(".c").unwrap());

        if cache.is_dirty(file, &hash) {
            println!("compiling {}", file);

            let status = Command::new("x86_64-elf-gcc")
                .args(["-ffreestanding", "-c", &src, "-o", &obj, &format!("-I{}", SRC)])
                .status()
                .unwrap();

            assert!(status.success());
        }
    });

    for file in files {
        let src = format!("{}/{}", SRC, file);
        let hash = hash_file(&src);
        cache.update(file, hash);
    }
}

fn build_asm(cache: &mut BuildCache, files: &[&str]) {
    std::fs::create_dir_all(".build/stage3").unwrap();

    files.par_iter().for_each(|file| {
        let src = format!("{}/{}", SRC, file);
        let hash = hash_file(&src);

        let obj = format!("{}/{}.o", OUT, file.strip_suffix(".asm").unwrap());

        if cache.is_dirty(file, &hash) {
            println!("assembling {}", file);

            let status =
                Command::new("nasm").args(["-f", "elf64", &src, "-o", &obj]).status().unwrap();

            assert!(status.success());
        }
    });

    for file in files {
        let src = format!("{}/{}", SRC, file);
        let hash = hash_file(&src);
        cache.update(file, hash);
    }
}

pub fn build_stage3() {
    let mut cache = BuildCache::load();

    let c_files = ["main.c", "fat32.c", "elf_loader.c", "debug.c", "disk_probe.c", "virtio_blk.c", "libc.c"];

    let asm_files = ["entry.asm"];

    build_c(&mut cache, &c_files);
    build_asm(&mut cache, &asm_files);

    let objs: Vec<String> = std::fs::read_dir(".build/stage3")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_string_lossy().to_string())
        .filter(|p| p.ends_with(".o"))
        .collect();

    let status = Command::new("x86_64-elf-ld")
        .args(&objs)
        .args([
            "-T",
            "bootloader/stage3/linker.ld",
            "-nostdlib",
            "--build-id=none",
            "-o",
            ".build/stage3.elf",
        ])
        .status()
        .unwrap();

    assert!(status.success());

    Command::new("x86_64-elf-objcopy")
        .args(["-O", "binary", ".build/stage3.elf", ".build/stage3.bin"])
        .status()
        .unwrap();

    cache.save();

    println!("stage3 incremental build done");
}
