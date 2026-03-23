use std::path::PathBuf;

fn main() {
    let kernel_bin = PathBuf::from(
        std::env::var("CARGO_BIN_FILE_KERNEL_kernel")
            .expect("kernel artifact not found — make sure bindeps is enabled")
    );

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let bios_path = out_dir.join("bios.img");

    bootloader::BiosBoot::new(&kernel_bin)
        .create_disk_image(&bios_path)
        .unwrap();

    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
}