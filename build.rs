use std::path::PathBuf;

fn main() {
    let kernel = PathBuf::from(std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel").unwrap());

    let uefi_path = PathBuf::from("target/uefi.img");
    let bios_path = PathBuf::from("target/bios.img");

    std::fs::create_dir_all("target").unwrap();

    bootloader::UefiBoot::new(&kernel).create_disk_image(&uefi_path).unwrap();
    bootloader::BiosBoot::new(&kernel).create_disk_image(&bios_path).unwrap();

    println!("cargo:rustc-env=UEFI_PATH={}", uefi_path.display());
    println!("cargo:rustc-env=BIOS_PATH={}", bios_path.display());
}
