use std::process::Command;

use crate::{
    cache::BuildCache,
    consts::{BUILD_DIR, STAGE1_SRC},
    hash::hash_file,
    run,
};

const KEY: &str = "stage1_build";

pub fn build(cache: &mut BuildCache) {
    let src = format!("{}/main.asm", STAGE1_SRC);
    let out = format!("{}/stage1.bin", BUILD_DIR);
    let hash = hash_file(&src);

    if cache.is_dirty(KEY, &hash) {
        println!("building stage1");

        run(Command::new("nasm").args(["-f", "bin", &src, "-o", &out]));

        cache.update(KEY, hash);
    }
}
