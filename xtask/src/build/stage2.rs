use std::process::Command;

use crate::{
    cache::BuildCache,
    consts::{BUILD_DIR, STAGE2_SRC},
    hash::hash_file,
    run,
};

const KEY: &str = "stage2_main.asm";

pub fn build(cache: &mut BuildCache) {
    let src = format!("{}/main.asm", STAGE2_SRC);
    let out = format!("{}/stage2.bin", BUILD_DIR);
    let hash = hash_file(&src);

    if cache.is_dirty(KEY, &hash) {
        println!("building stage2");

        run(Command::new("nasm").args(["-f", "bin", &src, "-o", &out]));

        cache.update(KEY, hash);
    }
}
