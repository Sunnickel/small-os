use std::process::Command;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    cache::BuildCache,
    consts::*,
    discover::{find_asm_files, find_c_files, find_rust_files},
    hash::hash_file,
};

pub fn format_all() {
    let mut cache = BuildCache::load();

    // Stage 1
    let stage1_asm = find_asm_files(STAGE1_SRC);
    format_asm(&mut cache, &stage1_asm);

    // Stage 2
    let stage2_asm = find_asm_files(STAGE2_SRC);
    format_asm(&mut cache, &stage2_asm);

    // Stage 3
    let stage3_asm = find_asm_files(STAGE3_SRC);
    let stage3_c = find_c_files(STAGE3_SRC);

    format_asm(&mut cache, &stage3_asm);
    format_c(&mut cache, &stage3_c);

    // Rust workspace
    for krate in WORKSPACE_CRATES {
        let rust_files = find_rust_files(krate);
        format_rust(&mut cache, &rust_files);
    }

    cache.save();
    println!("formatting done");
}

pub fn format_c(cache: &mut BuildCache, files: &[String]) {
    format_files(cache, files, "clang-format", &["-i"], "fmt:c");
}

pub fn format_asm(cache: &mut BuildCache, files: &[String]) {
    format_files(cache, files, "asmfmt", &[], "fmt:asm");
}

pub fn format_rust(cache: &mut BuildCache, files: &[String]) {
    format_files(cache, files, "rustfmt", &[], "fmt:rust");
}

fn format_files(
    cache: &mut BuildCache,
    files: &[String],
    tool: &str,
    args: &[&str],
    key_prefix: &str,
) {
    // ---------------------------
    // Phase 1: determine dirty files
    // ---------------------------
    let dirty: Vec<String> = files
        .iter()
        .filter(|src| {
            let key = format!("{}:{}", key_prefix, src);
            let hash = hash_file(src);
            cache.is_dirty(&key, &hash)
        })
        .cloned()
        .collect();

    // ---------------------------
    // Phase 2: format in parallel
    // ---------------------------
    dirty.par_iter().for_each(|src| {
        println!("formatting {}", src);

        let status = Command::new(tool)
            .args(args)
            .arg(src)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();

        if !status.success() {
            eprintln!("{} failed on {}", tool, src);
        }
    });

    // ---------------------------
    // Phase 3: update cache (serial, safe)
    // ---------------------------
    for src in dirty {
        let key = format!("{}:{}", key_prefix, src);
        let hash = hash_file(&src);
        cache.update(&key, hash);
    }
}
