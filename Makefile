NASM  := nasm
CARGO := cargo

BOOT_IMG      := $(OUT)/boot.img       # Installer boot disk
STAGE1_BIN    := $(OUT)/stage1.bin
STAGE2_BIN    := $(OUT)/stage2.bin
INSTALLER_ELF := $(OUT)/installer.elf
DISK_IMG      := $(OUT)/disk.img       # Target disk

BOOT_IMG_SIZE_MB := 16
DISK_IMG_SIZE_MB := 64

.PHONY: all
all: $(BOOT_IMG) $(DISK_IMG)
	@echo ""
	@echo "  Build complete."
	@echo "    Boot medium : $(BOOT_IMG)"
	@echo "    Target disk : $(DISK_IMG)"
	@echo ""
	@echo "  Run with:  make run"
	@echo ""

# =========================
# Stage1
# =========================
.PHONY: stage1
stage1: $(STAGE1_BIN)

$(STAGE1_BIN): bootloader/src/stage1.asm | $(OUT)
	$(NASM) -f bin $< -o $@
	@echo "  [stage1]  OK — 512 bytes"

# =========================
# Stage2
# =========================
.PHONY: stage2
stage2: $(STAGE2_BIN)

$(STAGE2_BIN): bootloader/src/stage2.asm | $(OUT)
	$(NASM) -f bin $< -o $@
	@echo "  [stage2]  OK — $$(stat -c%s $@) bytes"

# =========================
# Installer
# =========================
.PHONY: installer
installer: $(INSTALLER_ELF)

$(INSTALLER_ELF): | $(OUT)
	$(CARGO) build --package installer --release
	cp target/x86_64-unknown-none/release/installer $@
	@echo "  [installer]  OK — $$(stat -c%s $@) bytes"

# =========================
# Boot image (installer disk)
# =========================
.PHONY: boot-img
boot-img: $(BOOT_IMG)

$(BOOT_IMG): $(STAGE1_BIN) $(STAGE2_BIN) $(INSTALLER_ELF) | $(OUT)
	@echo "  [boot.img]  Allocating $(BOOT_IMG_SIZE_MB) MB..."
	dd if=/dev/zero of=$@ bs=1M count=$(BOOT_IMG_SIZE_MB) status=none

	@echo "  [boot.img]  Writing stage1 bootloader at LBA 0..."
	dd if=$(STAGE1_BIN) of=$@ bs=512 seek=0 conv=notrunc status=none

	@echo "  [boot.img]  Writing stage2 bootloader at LBA 1..."
	dd if=$(STAGE2_BIN) of=$@ bs=512 seek=1 conv=notrunc status=none

	@echo "  [boot.img]  Writing installer kernel at LBA 11..."
	dd if=$(INSTALLER_ELF) of=$@ bs=512 seek=11 conv=notrunc status=none

	@echo "  [boot.img]  Done."

# =========================
# Target disk (empty)
# =========================
.PHONY: disk-img
disk-img: $(DISK_IMG)

$(DISK_IMG): | $(OUT)
	@echo "  [disk.img]  Allocating $(DISK_IMG_SIZE_MB) MB blank target disk..."
	dd if=/dev/zero of=$@ bs=1M count=$(DISK_IMG_SIZE_MB) status=none
	@echo "  [disk.img]  Done."

# =========================
# Run QEMU
# =========================
.PHONY: run
run: $(BOOT_IMG) $(DISK_IMG)
	qemu-system-x86_64 \
	    -machine q35 \
	    -serial mon:stdio \
	    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
	    -drive id=boot,format=raw,if=none,file=$(BOOT_IMG) \
	    -device virtio-blk-pci,drive=boot,disable-legacy=on,disable-modern=off \
	    -drive id=disk,format=raw,if=none,file=$(DISK_IMG) \
	    -device virtio-blk-pci,drive=disk,disable-legacy=on,disable-modern=off

# =========================
# Clean
# =========================
.PHONY: clean
clean:
	$(CARGO) clean
	rm -rf $(OUT)
	@echo "  Cleaned."

$(OUT):
	mkdir -p $(OUT)