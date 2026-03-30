use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
};

// ── Layout (must match what the kernel expects)
// ───────────────────────────────

pub const SECTOR_SIZE: u64 = 512;
pub const DISK_SIZE_MB: u64 = 64;
pub const DISK_SIZE_BYTES: u64 = DISK_SIZE_MB * 1024 * 1024;
pub const TOTAL_SECTORS: u64 = DISK_SIZE_BYTES / SECTOR_SIZE;
pub const PARTITION_START_LBA: u64 = 34;
pub const PARTITION_END_LBA: u64 = TOTAL_SECTORS - 34;
pub const PARTITION_BYTE_OFFSET: u64 = PARTITION_START_LBA * SECTOR_SIZE;
pub const PARTITION_BYTE_SIZE: u64 = (PARTITION_END_LBA - PARTITION_START_LBA + 1) * SECTOR_SIZE;

// ── Entry point
// ───────────────────────────────────────────────────────────────

pub fn create_ntfs_disk(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Allocating {}MB image...", DISK_SIZE_MB);
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?
        .set_len(DISK_SIZE_BYTES)?;

    write_gpt(path)?;

    eprintln!("Detecting platform for NTFS formatter...");
    match detect_platform() {
        Platform::Windows => format_via_wsl(path)?,
        Platform::Linux => format_native(path)?,
        Platform::Mac => format_via_brew(path)?,
        Platform::Unknown => {
            return Err("Cannot format NTFS: unsupported platform.\n\
             Install ntfs-3g (Linux/Mac) or enable WSL (Windows)."
                .into());
        }
    }

    verify(path)?;
    eprintln!("Disk image ready: {path}");
    Ok(())
}

// ── Platform detection
// ────────────────────────────────────────────────────────

enum Platform {
    Windows,
    Linux,
    Mac,
    Unknown,
}

fn detect_platform() -> Platform {
    if cfg!(target_os = "windows") {
        // Confirm WSL is actually available before committing to it.
        if Command::new("wsl").arg("--status").output().map(|o| o.status.success()).unwrap_or(false)
        {
            return Platform::Windows;
        }
        return Platform::Unknown;
    }
    if cfg!(target_os = "linux") {
        if which("mkntfs").or_else(|| which("mkfs.ntfs")).is_some() {
            return Platform::Linux;
        }
        return Platform::Unknown;
    }
    if cfg!(target_os = "macos") {
        if which("mkntfs").is_some() {
            return Platform::Mac;
        }
        return Platform::Unknown;
    }
    Platform::Unknown
}

fn which(cmd: &str) -> Option<String> {
    Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ── Formatters
// ────────────────────────────────────────────────────────────────

/// Windows: run mkntfs inside WSL against a loop-mounted partition region.
fn format_via_wsl(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Canonicalize to an absolute Windows path first, then convert to WSL.
    let abs = std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot resolve path '{}': {}", path, e))?;
    let abs_str = abs.to_string_lossy().to_string();
    let wsl_path = windows_to_wsl_path(&abs_str)?;
    let script = mkntfs_script(&wsl_path, "sudo");
    eprintln!("Formatting via WSL (path: {wsl_path})");
    run_wsl_script(&script)
}

/// Native Linux: same script, no path conversion needed.
fn format_native(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // On Linux the path is already a Unix path.
    // Prefer mkntfs (ntfs-3g package name) then mkfs.ntfs (alias).
    let mkntfs = which("mkntfs").unwrap_or_else(|| "mkfs.ntfs".into());
    let sudo = if noelevation_needed() { "" } else { "sudo" };
    let script = mkntfs_script_with_tool(path, sudo, &mkntfs);
    eprintln!("Formatting natively (tool: {mkntfs})");
    run_sh_script(&script)
}

/// macOS: ntfs-3g from Homebrew provides mkntfs.
fn format_via_brew(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let script = mkntfs_script_with_tool(path, "sudo", "mkntfs");
    eprintln!("Formatting via Homebrew ntfs-3g...");
    run_sh_script(&script)
}

// ── Shell scripts
// ─────────────────────────────────────────────────────────────

/// Generate the loop-device + mkntfs shell script.
fn mkntfs_script(img: &str, sudo: &str) -> String { mkntfs_script_with_tool(img, sudo, "mkntfs") }

fn mkntfs_script_with_tool(img: &str, sudo: &str, tool: &str) -> String {
    format!(
        r#"set -euo pipefail
IMG="{img}"
OFFSET={PARTITION_BYTE_OFFSET}
SIZE={PARTITION_BYTE_SIZE}
PART_START={PARTITION_START_LBA}

[ -f "$IMG" ] || {{ echo "ERROR: $IMG not found" >&2; exit 1; }}

echo "[ntfs] setting up loop device..."
LOOP=$({sudo} losetup -f --show --offset="$OFFSET" --sizelimit="$SIZE" "$IMG")
echo "[ntfs] loop: $LOOP"

cleanup() {{ {sudo} losetup -d "$LOOP" 2>/dev/null || true; }}
trap cleanup EXIT

echo "[ntfs] formatting..."
{sudo} {tool} \
    -F \
    -L KRNLDISK \
    -s 512 \
    -c 512  \
    -p "$PART_START" \
    -H 255 \
    -S 63 \
    -Q \
    "$LOOP"

echo "[ntfs] done."
"#
    )
}

fn run_wsl_script(script: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("wsl")
        .args(["-e", "bash", "-c", script])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        // Give a useful hint if ntfs-3g isn't installed in WSL yet.
        return Err("WSL script failed.\n\
             Make sure ntfs-3g is installed in WSL: sudo apt install ntfs-3g\n\
             And that losetup/mkntfs don't require a password: \
             see sudoers setup in the README."
            .into());
    }
    Ok(())
}

fn run_sh_script(script: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("bash")
        .args(["-c", script])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        return Err("mkntfs script failed.\n\
             Make sure ntfs-3g is installed:\n\
             - Linux: sudo apt install ntfs-3g\n\
             - macOS: brew install ntfs-3g"
            .into());
    }
    Ok(())
}

/// On Linux, check if we're already root (CI environments, containers).
fn noelevation_needed() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|uid| uid == 0)
        .unwrap_or(false)
}

// ── Path conversion
// ───────────────────────────────────────────────────────────

fn windows_to_wsl_path(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Use wslpath for reliable conversion — handles edge cases like OneDrive
    // paths, spaces, UNC paths, etc.  Much safer than doing it by hand.
    let output = Command::new("wsl").args(["-e", "wslpath", "-u", path]).output()?;

    if output.status.success() {
        let wsl = String::from_utf8(output.stdout)?.trim().to_string();
        if !wsl.is_empty() {
            return Ok(wsl);
        }
    }

    // Fallback: manual conversion for simple C:\foo\bar paths.
    let abs = std::fs::canonicalize(path)?;
    let s = abs.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    if s.len() >= 2 && s.chars().nth(1) == Some(':') {
        let drive = s.chars().next().unwrap().to_ascii_lowercase();
        let rest = s[2..].replace('\\', "/");
        return Ok(format!("/mnt/{drive}{rest}"));
    }
    Err(format!("Cannot convert path to WSL format: {path}").into())
}

// ── GPT ───────────────────────────────────────────────────────────────────────

pub fn write_gpt(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Protective MBR
    let mut mbr = [0u8; 512];
    mbr[0..3].copy_from_slice(&[0xEB, 0xFE, 0x90]);
    let e = &mut mbr[0x1BE..0x1CE];
    e[0] = 0x00;
    e[1..4].copy_from_slice(&[0x00, 0x02, 0x00]);
    e[4] = 0xEE;
    e[5..8].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
    e[8..12].copy_from_slice(&1u32.to_le_bytes());
    e[12..16].copy_from_slice(&((TOTAL_SECTORS - 1).min(0xFFFF_FFFF) as u32).to_le_bytes());
    mbr[0x1FE] = 0x55;
    mbr[0x1FF] = 0xAA;
    write_at(path, 0, &mbr)?;

    // Partition entry array
    let mut entry = [0u8; 128];
    // Microsoft Basic Data GUID {EBD0A0A2-B9E5-4433-87C0-68B6B72699C7}
    entry[0..16].copy_from_slice(&[
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
        0xC7,
    ]);
    entry[16..32].copy_from_slice(&[
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x47, 0x78, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
        0x00,
    ]);
    entry[32..40].copy_from_slice(&PARTITION_START_LBA.to_le_bytes());
    entry[40..48].copy_from_slice(&PARTITION_END_LBA.to_le_bytes());
    for (i, c) in "KRNLDISK".chars().enumerate() {
        let b = (c as u16).to_le_bytes();
        entry[56 + i * 2] = b[0];
        entry[56 + i * 2 + 1] = b[1];
    }
    let mut arr = vec![0u8; 128 * 128];
    arr[..128].copy_from_slice(&entry);
    let entries_crc = crc32(&arr);

    // Primary header (LBA 1)
    let last_lba = TOTAL_SECTORS - 1;
    let sec_entries_start = last_lba - 32;
    let primary = gpt_header(1, last_lba, 2, entries_crc);
    write_at(path, SECTOR_SIZE, &primary)?;
    write_at(path, 2 * SECTOR_SIZE, &arr)?;

    // Secondary header
    let secondary = gpt_header(last_lba, 1, sec_entries_start, entries_crc);
    write_at(path, sec_entries_start * SECTOR_SIZE, &arr)?;
    write_at(path, last_lba * SECTOR_SIZE, &secondary)?;

    eprintln!("GPT written");
    Ok(())
}

fn gpt_header(my_lba: u64, alt_lba: u64, entries_lba: u64, entries_crc: u32) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    let disk_guid = [
        0xA1u8, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x47, 0x18, 0x89, 0x9A, 0xAB, 0xCD, 0xEF, 0x01, 0x23,
        0x45,
    ];
    h[0..8].copy_from_slice(b"EFI PART");
    h[8..12].copy_from_slice(&[0x00, 0x00, 0x01, 0x00]);
    h[12..16].copy_from_slice(&92u32.to_le_bytes());
    h[24..32].copy_from_slice(&my_lba.to_le_bytes());
    h[32..40].copy_from_slice(&alt_lba.to_le_bytes());
    h[40..48].copy_from_slice(&PARTITION_START_LBA.to_le_bytes());
    h[48..56].copy_from_slice(&(TOTAL_SECTORS - 34).to_le_bytes());
    h[56..72].copy_from_slice(&disk_guid);
    h[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    h[80..84].copy_from_slice(&128u32.to_le_bytes());
    h[84..88].copy_from_slice(&128u32.to_le_bytes());
    h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let crc = crc32(&h[..92]);
    h[16..20].copy_from_slice(&crc.to_le_bytes());
    h
}

// ── Verification
// ──────────────────────────────────────────────────────────────

fn verify(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 512];
    let mut f = fs::File::open(path)?;
    f.seek(SeekFrom::Start(PARTITION_BYTE_OFFSET))?;
    f.read_exact(&mut buf)?;

    if &buf[3..11] != b"NTFS    " {
        return Err(format!(
            "NTFS verification failed — bad OEM ID: {:02X?}\n\
             Expected: 4E 54 46 53 20 20 20 20",
            &buf[3..11]
        )
        .into());
    }

    let bps = u16::from_le_bytes([buf[0x0B], buf[0x0C]]);
    let spc = buf[0x0D];
    let mft = u64::from_le_bytes(buf[0x30..0x38].try_into().unwrap());
    let serial = u64::from_le_bytes(buf[0x48..0x50].try_into().unwrap());

    eprintln!("✓ NTFS verified");
    eprintln!("  bytes/sector    : {bps}");
    eprintln!("  sectors/cluster : {spc}");
    eprintln!(
        "  MFT LCN         : {mft}  (disk offset: {:#X})",
        PARTITION_BYTE_OFFSET + mft * bps as u64 * spc as u64
    );
    eprintln!("  serial number   : {serial:#018X}");

    Ok(())
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

fn write_at(path: &str, offset: u64, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fs::OpenOptions::new().write(true).open(path)?;
    f.seek(SeekFrom::Start(offset))?;
    f.write_all(data)?;
    f.flush()?;
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    const POLY: u32 = 0xEDB8_8320;
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ POLY } else { crc >> 1 };
        }
    }
    !crc
}
