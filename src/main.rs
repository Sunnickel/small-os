use std::path::PathBuf;

fn main() {
    let bios_path = env!("BIOS_PATH");

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd
        .arg("-drive").arg(format!("format=raw,file={bios_path}"))
        .arg("-device").arg("isa-debug-exit,iobase=0xf4,iosize=0x04")
        .arg("-serial").arg("stdio")
        .arg("-no-reboot");

    let mut child = cmd.spawn().unwrap_or_else(|e| {
        eprintln!("Failed to launch QEMU: {e}");
        std::process::exit(1);
    });

    let status = child.wait().unwrap();


    let stable_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bios.img");
    std::fs::copy(&bios_path, &stable_path).unwrap();
    println!("cargo:rerun-if-changed=build.rs");


    match status.code() {
        Some(33) => {
            println!("Tests passed!");
            std::process::exit(0);
        }
        Some(35) => {
            println!("Tests failed!");
            std::process::exit(1);
        }
        other => {
            println!("QEMU exited with unexpected code: {other:?}");
            std::process::exit(1);
        }
    }
}