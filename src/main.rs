use std::{
    env,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
};

use ovmf_prebuilt::{Arch, FileType, Prebuilt, Source};

fn main() {
    let uefi_path = env!("UEFI_PATH");
    let bios_path = env!("BIOS_PATH");

    let args: Vec<String> = env::args().collect();
    let prog = &args[0];

    let uefi = match args.get(1).map(|s| s.to_lowercase()) {
        Some(ref s) if s == "uefi" => true,
        Some(ref s) if s == "bios" => false,
        Some(ref s) if s == "-h" || s == "--help" => {
            println!("Usage: {prog} [uefi|bios] [virtio|ahci]");
            println!("  uefi   - boot using OVMF (UEFI)");
            println!("  bios   - boot using legacy BIOS");
            println!("  virtio - attach disk as VirtIO block device (default)");
            println!("  ahci   - attach disk as AHCI/SATA device");
            std::process::exit(0);
        }
        _ => {
            eprintln!("Usage: {prog} [uefi|bios] [virtio|ahci]");
            std::process::exit(1);
        }
    };

    let use_ahci = matches!(args.get(2).map(|s| s.to_lowercase()).as_deref(), Some("ahci"));

    let disk_path = "target/disk.img";

    if !Path::new(disk_path).exists() {
        eprintln!("Creating new MBR/NTFS disk image at {disk_path}...");
        create_mbr_ntfs_disk(disk_path, 64).expect("Failed to create disk");
        eprintln!("Successfully created MBR disk with NTFS partition");
    }

    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.arg("-serial").arg("mon:stdio");
    cmd.arg("-device").arg("isa-debug-exit,iobase=0xf4,iosize=0x04");

    // Attach disk with proper cache settings
    if use_ahci {
        cmd.arg("-device").arg("ich9-ahci,id=ahci");
        cmd.arg("-drive")
            .arg(format!("id=disk0,file={disk_path},format=raw,if=none,cache=writethrough"));
        cmd.arg("-device").arg("ide-hd,drive=disk0,bus=ahci.0");
    } else {
        cmd.arg("-drive")
            .arg(format!("id=disk0,file={disk_path},format=raw,if=none,cache=writethrough"));
        cmd.arg("-device").arg("virtio-blk-pci,drive=disk0,disable-legacy=on,disable-modern=off");
    }

    if uefi {
        let prebuilt =
            Prebuilt::fetch(Source::LATEST, "target/ovmf").expect("failed to update prebuilt");

        let code = prebuilt.get_file(Arch::X64, FileType::Code);
        let vars = prebuilt.get_file(Arch::X64, FileType::Vars);

        cmd.arg("-drive").arg(format!("format=raw,file={uefi_path}"));
        cmd.arg("-drive")
            .arg(format!("if=pflash,format=raw,unit=0,file={},readonly=on", code.display()));
        cmd.arg("-drive")
            .arg(format!("if=pflash,format=raw,unit=1,file={},snapshot=on", vars.display()));
    } else {
        cmd.arg("-drive").arg(format!("format=raw,file={bios_path}"));
    }

    let mut child = cmd.spawn().expect("failed to start qemu-system-x86_64");
    let status = child.wait().expect("failed to wait on qemu");
    match status.code().unwrap_or(1) {
        0x10 => std::process::exit(0),
        0x11 => std::process::exit(1),
        _ => std::process::exit(2),
    };
}

/// Creates a simple MBR disk with NTFS at LBA 1 (offset 512)
fn create_mbr_ntfs_disk(path: &str, size_mb: u32) -> Result<(), Box<dyn std::error::Error>> {
    let sector_size = 512u64;
    let size_bytes = (size_mb as u64) * 1024 * 1024;
    let total_sectors = size_bytes / sector_size;

    // MBR layout: LBA 0 = MBR, LBA 1+ = NTFS
    let partition_start = 1u64;
    let partition_sectors = total_sectors - 1;

    eprintln!(
        "Creating {}MB MBR disk ({} sectors, NTFS at LBA {})",
        size_mb, total_sectors, partition_start
    );

    // Create raw file filled with zeroes
    {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)?;

        file.set_len(size_bytes)?;

        let zero_chunk = vec![0u8; 1024 * 1024];
        let chunks = (size_bytes / (1024 * 1024)) as usize;
        for _ in 0..chunks {
            file.write_all(&zero_chunk)?;
        }
        file.flush()?;
    }

    // Write MBR with single NTFS partition
    write_mbr(path, partition_start, partition_sectors)?;

    // Format NTFS at offset 512 (LBA 1)
    format_ntfs_partition(path, partition_start, partition_sectors)?;

    // Verify NTFS was written correctly
    verify_ntfs_boot_sector(path, partition_start)?;

    Ok(())
}

fn write_mbr(
    path: &str,
    start_lba: u64,
    sector_count: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut mbr = vec![0u8; 512];

    // Boot code: simple infinite loop if executed
    mbr[0..3].copy_from_slice(&[0xEB, 0xFE, 0x90]);

    // Partition entry 1 (offset 0x1BE = 446)
    mbr[0x1BE] = 0x80; // Bootable
    mbr[0x1BF] = 0x01; // CHS start head
    mbr[0x1C0] = 0x01; // CHS start sector
    mbr[0x1C1] = 0x00;
    mbr[0x1C2] = 0x07; // NTFS partition type

    // CHS end
    mbr[0x1C3] = 0xFE;
    mbr[0x1C4] = 0xFF;
    mbr[0x1C5] = 0xFF;

    // LBA start
    mbr[0x1C6..0x1CA].copy_from_slice(&(start_lba as u32).to_le_bytes());
    // LBA count (cap at 0xFFFFFFFF)
    let size = if sector_count > u32::MAX as u64 { 0xFFFFFFFFu32 } else { sector_count as u32 };
    mbr[0x1CA..0x1CE].copy_from_slice(&size.to_le_bytes());

    // Boot signature
    mbr[0x1FE] = 0x55;
    mbr[0x1FF] = 0xAA;

    let mut file = fs::OpenOptions::new().write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&mbr)?;
    file.flush()?;

    eprintln!("MBR written: NTFS partition at LBA {} ({} sectors)", start_lba, sector_count);
    Ok(())
}

fn format_ntfs_partition(
    path: &str,
    start_lba: u64,
    sector_count: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_obj = Path::new(path);
    let abs_path = fs::canonicalize(path_obj)?;
    let abs_path_str = abs_path.to_str().ok_or("Invalid path")?;

    // Convert Windows path to WSL path
    let wsl_path = if abs_path_str.starts_with(r"\\?\") {
        let without_prefix = &abs_path_str[4..];
        let drive = without_prefix.chars().next().unwrap().to_lowercase().to_string();
        let rest = &without_prefix[2..].replace('\\', "/");
        format!("/mnt/{}{}", drive, rest)
    } else if abs_path_str.len() > 2 && abs_path_str.chars().nth(1) == Some(':') {
        let drive = abs_path_str.chars().next().unwrap().to_lowercase().to_string();
        let rest = &abs_path_str[2..].replace('\\', "/");
        format!("/mnt/{}{}", drive, rest)
    } else {
        abs_path_str.replace('\\', "/")
    };

    let start_byte = start_lba * 512;
    let size_bytes = sector_count * 512;

    // Use sudo for losetup and mkfs.ntfs
    let script = format!(
        r#"
set -e
IMG="{wsl_path}"
OFFSET={start_byte}
SIZE={size_bytes}

echo "Using WSL path: $IMG"
echo "Setting up loop device at offset $OFFSET, size $SIZE..."

# Check if file exists
if [ ! -f "$IMG" ]; then
    echo "ERROR: File not found: $IMG"
    exit 1
fi

# Use sudo for losetup
LOOP=$(sudo losetup -f --show --offset=$OFFSET --sizelimit=$SIZE "$IMG")

echo "Formatting as NTFS..."
sudo mkfs.ntfs -F -L KRNLDISK -s 512 "$LOOP"

echo "Detaching loop device..."
sudo losetup -d "$LOOP"
echo "Done."
"#
    );

    eprintln!("Formatting NTFS partition via WSL (requires sudo)...");
    eprintln!("WSL path: {}", wsl_path);

    let status = Command::new("wsl")
        .arg("-e")
        .arg("bash")
        .arg("-c")
        .arg(&script)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(s) if s.success() => {
            eprintln!("NTFS formatting complete");
            Ok(())
        }
        Ok(_) => Err("WSL formatting script failed (maybe sudo needs password?)".into()),
        Err(e) => Err(format!("WSL not available: {}", e).into()),
    }
}

fn verify_ntfs_boot_sector(
    path: &str,
    partition_lba: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let offset = partition_lba * 512;
    let mut buf = [0u8; 512];

    let mut file = fs::OpenOptions::new().read(true).open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;

    // Check for NTFS signature
    if &buf[3..11] == b"NTFS    " {
        eprintln!(
            "✓ NTFS boot sector verified at offset {} (OEM ID: {:02x?})",
            offset,
            &buf[3..11]
        );

        // Print some key fields
        let bytes_per_sector = u16::from_le_bytes([buf[0x0B], buf[0x0C]]);
        let sectors_per_cluster = buf[0x0D];
        let total_sectors = u64::from_le_bytes([
            buf[0x28], buf[0x29], buf[0x2A], buf[0x2B], buf[0x2C], buf[0x2D], buf[0x2E], buf[0x2F],
        ]);
        let mft_start = u64::from_le_bytes([
            buf[0x30], buf[0x31], buf[0x32], buf[0x33], buf[0x34], buf[0x35], buf[0x36], buf[0x37],
        ]);

        eprintln!("  Bytes per sector: {}", bytes_per_sector);
        eprintln!("  Sectors per cluster: {}", sectors_per_cluster);
        eprintln!("  Total sectors: {}", total_sectors);
        eprintln!("  MFT start cluster: {}", mft_start);

        Ok(())
    } else {
        eprintln!("✗ Invalid boot sector at offset {}", offset);
        eprintln!("  Expected: NTFS    (4E 54 46 53 20 20 20 20)");
        eprintln!("  Got:      {:02x?}", &buf[3..11]);
        Err("NTFS verification failed".into())
    }
}
