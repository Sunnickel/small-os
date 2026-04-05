#!/usr/bin/env bash
set -euo pipefail

# ── Layout ────────────────────────────────
SECTOR_SIZE=512
DISK_SIZE_MB=64
DISK_SIZE_BYTES=$(( DISK_SIZE_MB * 1024 * 1024 ))
TOTAL_SECTORS=$(( DISK_SIZE_BYTES / SECTOR_SIZE ))
PARTITION_START_LBA=34
PARTITION_END_LBA=$(( TOTAL_SECTORS - 34 ))
PARTITION_BYTE_OFFSET=$(( PARTITION_START_LBA * SECTOR_SIZE ))
PARTITION_BYTE_SIZE=$(( (PARTITION_END_LBA - PARTITION_START_LBA + 1) * SECTOR_SIZE ))

DISK_PATH="${1:-target/disk.img}"

# ── Helpers ────────────────────────────────────────────────────────────────────

die() { echo "ERROR: $*" >&2; exit 1; }

requires() {
    command -v "$1" &>/dev/null || die "'$1' not found. $2"
}

# Write a raw byte string (as hex pairs) to a file at a given offset.
write_bytes() {
    local file="$1" offset="$2" hex="$3"
    printf '%b' "$(echo "$hex" | sed 's/../\\x&/g')" \
        | dd of="$file" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

# Write a 32-bit little-endian value to a file.
write_u32_le() {
    local file="$1" offset="$2" value="$3"
    printf '%08x' "$value" \
        | awk '{print substr($0,7,2) substr($0,5,2) substr($0,3,2) substr($0,1,2)}' \
        | xxd -r -p \
        | dd of="$file" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

# Write a 64-bit little-endian value to a file.
write_u64_le() {
    local file="$1" offset="$2" value="$3"
    printf '%016x' "$value" \
        | awk '{print substr($0,15,2) substr($0,13,2) substr($0,11,2) substr($0,9,2) \
                       substr($0,7,2)  substr($0,5,2)  substr($0,3,2)  substr($0,1,2)}' \
        | xxd -r -p \
        | dd of="$file" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

# CRC32 of a byte range using python3 (widely available, avoids extra deps).
crc32_file_range() {
    local file="$1" offset="$2" length="$3"
    python3 - "$file" "$offset" "$length" <<'EOF'
import sys, struct, zlib
f, off, length = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
with open(f, 'rb') as fh:
    fh.seek(off)
    data = fh.read(length)
print(zlib.crc32(data) & 0xFFFFFFFF)
EOF
}

# ── Step 1: Allocate raw image ─────────────────────────────────────────────────

echo "Allocating ${DISK_SIZE_MB}MB image at '${DISK_PATH}'..."
mkdir -p "$(dirname "$DISK_PATH")"
dd if=/dev/zero of="$DISK_PATH" bs=1M count="$DISK_SIZE_MB" status=none
echo "  done."

# ── Step 2: Write Protective MBR (LBA 0) ──────────────────────────────────────

echo "Writing protective MBR..."

# Jump + NOP signature
write_bytes "$DISK_PATH" 0 "EBFE90"

# Partition entry at 0x1BE: status=0x00, type=0xEE (GPT protective), start LBA=1
# Offsets into the 512-byte MBR sector:
#   0x1BE = 446 decimal
python3 - "$DISK_PATH" "$TOTAL_SECTORS" <<'EOF'
import sys, struct
path, total = sys.argv[1], int(sys.argv[2])
with open(path, 'r+b') as f:
    # Partition entry at 0x1BE
    f.seek(0x1BE)
    entry = bytearray(16)
    entry[0]    = 0x00          # status
    entry[1:4]  = b'\x00\x02\x00'  # CHS first (ignored)
    entry[4]    = 0xEE          # type: GPT protective
    entry[5:8]  = b'\xFF\xFF\xFF'  # CHS last (ignored)
    struct.pack_into('<I', entry, 8,  1)                           # start LBA
    struct.pack_into('<I', entry, 12, min(total - 1, 0xFFFFFFFF)) # size in LBAs
    f.write(entry)
    # Boot signature
    f.seek(0x1FE)
    f.write(b'\x55\xAA')
EOF

echo "  done."

# ── Step 3: Write GPT headers + partition entry array ─────────────────────────

echo "Writing GPT..."

ENTRIES_CRC=$(python3 - "$PARTITION_START_LBA" "$PARTITION_END_LBA" <<'EOF'
import sys, zlib, struct

start_lba = int(sys.argv[1])
end_lba   = int(sys.argv[2])

# Build the 128-entry partition array (128 bytes each = 16384 bytes total)
arr = bytearray(128 * 128)

# Entry 0: Microsoft Basic Data partition
# Type GUID {EBD0A0A2-B9E5-4433-87C0-68B6B72699C7} in mixed-endian
type_guid = bytes([
    0xA2,0xA0,0xD0,0xEB, 0xE5,0xB9, 0x33,0x44,
    0x87,0xC0, 0x68,0xB6,0xB7,0x26,0x99,0xC7,
])
# Unique partition GUID (arbitrary)
part_guid = bytes([
    0x11,0x22,0x33,0x44, 0x55,0x66, 0x47,0x78,
    0x99,0xAA, 0xBB,0xCC,0xDD,0xEE,0xFF,0x00,
])

arr[0:16]  = type_guid
arr[16:32] = part_guid
struct.pack_into('<Q', arr, 32, start_lba)
struct.pack_into('<Q', arr, 40, end_lba)

# Partition name "KRNLDISK" in UTF-16LE starting at offset 56
name = "KRNLDISK"
for i, ch in enumerate(name):
    arr[56 + i*2] = ord(ch)
    arr[56 + i*2 + 1] = 0x00

crc = zlib.crc32(arr) & 0xFFFFFFFF
print(crc)
EOF
)

LAST_LBA=$(( TOTAL_SECTORS - 1 ))
SEC_ENTRIES_START=$(( LAST_LBA - 32 ))

# Write primary + secondary GPT headers and entry arrays
python3 - \
    "$DISK_PATH" \
    "$TOTAL_SECTORS" \
    "$PARTITION_START_LBA" \
    "$PARTITION_END_LBA" \
    "$ENTRIES_CRC" \
    <<'EOF'
import sys, struct, zlib

path            = sys.argv[1]
total_sectors   = int(sys.argv[2])
part_start      = int(sys.argv[3])
part_end        = int(sys.argv[4])
entries_crc     = int(sys.argv[5])
sector          = 512
last_lba        = total_sectors - 1
sec_entries_lba = last_lba - 32

# ── Rebuild partition array ──────────────────────────────────────────────────
arr = bytearray(128 * 128)
type_guid = bytes([0xA2,0xA0,0xD0,0xEB,0xE5,0xB9,0x33,0x44,
                   0x87,0xC0,0x68,0xB6,0xB7,0x26,0x99,0xC7])
part_guid = bytes([0x11,0x22,0x33,0x44,0x55,0x66,0x47,0x78,
                   0x99,0xAA,0xBB,0xCC,0xDD,0xEE,0xFF,0x00])
arr[0:16]  = type_guid
arr[16:32] = part_guid
struct.pack_into('<Q', arr, 32, part_start)
struct.pack_into('<Q', arr, 40, part_end)
for i, ch in enumerate("KRNLDISK"):
    arr[56 + i*2] = ord(ch)

disk_guid = bytes([0xA1,0xB2,0xC3,0xD4,0xE5,0xF6,0x47,0x18,
                   0x89,0x9A,0xAB,0xCD,0xEF,0x01,0x23,0x45])

def make_header(my_lba, alt_lba, entries_lba, entries_crc):
    h = bytearray(512)
    h[0:8]   = b'EFI PART'
    h[8:12]  = b'\x00\x00\x01\x00'      # revision 1.0
    struct.pack_into('<I', h, 12, 92)    # header size
    struct.pack_into('<Q', h, 24, my_lba)
    struct.pack_into('<Q', h, 32, alt_lba)
    struct.pack_into('<Q', h, 40, part_start)
    struct.pack_into('<Q', h, 48, total_sectors - 34)
    h[56:72] = disk_guid
    struct.pack_into('<Q', h, 72, entries_lba)
    struct.pack_into('<I', h, 80, 128)   # num entries
    struct.pack_into('<I', h, 84, 128)   # entry size
    struct.pack_into('<I', h, 88, entries_crc)
    crc = zlib.crc32(h[:92]) & 0xFFFFFFFF
    struct.pack_into('<I', h, 16, crc)
    return h

primary   = make_header(1,        last_lba,        2,               entries_crc)
secondary = make_header(last_lba, 1,               sec_entries_lba, entries_crc)

with open(path, 'r+b') as f:
    f.seek(1 * sector);            f.write(primary)
    f.seek(2 * sector);            f.write(arr)
    f.seek(sec_entries_lba * sector); f.write(arr)
    f.seek(last_lba * sector);     f.write(secondary)

print("GPT written.")
EOF

echo "  done."

# ── Step 4: Format partition as NTFS ──────────────────────────────────────────

echo "Detecting platform for NTFS formatter..."

format_ntfs() {
    local tool sudo_prefix loop_dev

    # Determine sudo usage (skip if already root)
    if [ "$(id -u)" -eq 0 ]; then
        sudo_prefix=""
    else
        sudo_prefix="sudo"
    fi

    # Find mkntfs binary
    if command -v mkntfs &>/dev/null; then
        tool="mkntfs"
    elif command -v mkfs.ntfs &>/dev/null; then
        tool="mkfs.ntfs"
    else
        die "mkntfs/mkfs.ntfs not found. Install ntfs-3g:\n  apt install ntfs-3g  OR  brew install ntfs-3g"
    fi

    echo "  Using: $tool (offset=$PARTITION_BYTE_OFFSET size=$PARTITION_BYTE_SIZE)"

    loop_dev=$($sudo_prefix losetup -f --show \
        --offset="$PARTITION_BYTE_OFFSET" \
        --sizelimit="$PARTITION_BYTE_SIZE" \
        "$DISK_PATH")
    echo "  Loop device: $loop_dev"

    cleanup_loop() { $sudo_prefix losetup -d "$loop_dev" 2>/dev/null || true; }
    trap cleanup_loop EXIT

    $sudo_prefix "$tool" \
        -F \
        -L KRNLDISK \
        -s 512 \
        -c 4096 \
        -Q \
        "$loop_dev"

    $sudo_prefix losetup -d "$loop_dev"
    trap - EXIT
}

case "$(uname -s)" in
    Linux)
        format_ntfs
        ;;
    Darwin)
        requires mkntfs "Install via: brew install ntfs-3g"
        format_ntfs
        ;;
    MINGW*|MSYS*|CYGWIN*)
        # Windows: delegate to WSL
        requires wsl "Enable WSL: https://learn.microsoft.com/en-us/windows/wsl/install"
        WSL_PATH=$(wsl -e wslpath -u "$(pwd)/$DISK_PATH")
        wsl -e bash -c "
            set -euo pipefail
            LOOP=\$(sudo losetup -f --show \
                --offset=$PARTITION_BYTE_OFFSET \
                --sizelimit=$PARTITION_BYTE_SIZE \
                '$WSL_PATH')
            echo \"Loop: \$LOOP\"
            trap \"sudo losetup -d \$LOOP\" EXIT
            sudo mkntfs -F -L KRNLDISK -s 512 -c 4096 -Q \"\$LOOP\"
        "
        ;;
    *)
        die "Unsupported platform: $(uname -s)"
        ;;
esac

# ── Step 5: Verify ────────────────────────────────────────────────────────────

echo "Verifying NTFS..."
python3 - "$DISK_PATH" "$PARTITION_BYTE_OFFSET" <<'EOF'
import sys, struct

path, offset = sys.argv[1], int(sys.argv[2])
with open(path, 'rb') as f:
    f.seek(offset)
    buf = f.read(512)

oem = buf[3:11]
if oem != b'NTFS    ':
    print(f"FAIL: bad OEM ID: {oem.hex()}", file=sys.stderr)
    sys.exit(1)

bps     = struct.unpack_from('<H', buf, 0x0B)[0]
spc     = buf[0x0D]
mft_lcn = struct.unpack_from('<Q', buf, 0x30)[0]
serial  = struct.unpack_from('<Q', buf, 0x48)[0]

print("✓ NTFS verified")
print(f"  bytes/sector    : {bps}")
print(f"  sectors/cluster : {spc}")
print(f"  MFT LCN         : {mft_lcn}  (disk offset: {offset + mft_lcn * bps * spc:#X})")
print(f"  serial number   : {serial:#018X}")
EOF

echo ""
echo "Disk image ready: $DISK_PATH"