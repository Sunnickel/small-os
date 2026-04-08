NASM  := nasm
CARGO := cargo
OBJCOPY := rust-objcopy

OUT := .build

BOOT_IMG      := $(OUT)/boot.img
STAGE1_BIN    := $(OUT)/stage1.bin
STAGE2_BIN    := $(OUT)/stage2.bin
INSTALLER_ELF := $(OUT)/installer.elf
INSTALLER_BIN := $(OUT)/installer.bin
DISK_IMG      := $(OUT)/disk.img

BOOT_IMG_SIZE_MB := 16
DISK_IMG_SIZE_MB := 64

.PHONY: all
all: $(BOOT_IMG) $(DISK_IMG)
	@echo ""
	@echo "  Build complete."
	@echo "    Boot medium : $(BOOT_IMG)"
	@echo "    Target disk : $(DISK_IMG)"
	@echo ""
	@echo "  Run with: make run"
	@echo ""

# =========================
# Stage1
# =========================
.PHONY: stage1
stage1: $(STAGE1_BIN)

$(STAGE1_BIN): bootloader/src/stage1.asm | $(OUT)
	$(NASM) -f bin $< -o $@
	@echo "  [stage1] OK — 512 bytes"

# =========================
# Stage2
# =========================
.PHONY: stage2
stage2: $(STAGE2_BIN)

$(STAGE2_BIN): bootloader/src/stage2.asm | $(OUT)
	$(NASM) -f bin $< -o $@
	@echo "  [stage2] OK — $$(stat -c%s $@) bytes"

# =========================
# Installer (Rust)
# =========================
.PHONY: installer
installer: $(INSTALLER_ELF)

$(INSTALLER_BIN): $(OUT)
	$(CARGO) build --package installer --release
	rust-objcopy -O binary target/x86_64-unknown-none/release/installer $(INSTALLER_BIN)
	@echo "  [installer.bin] OK — $$(stat -c%s $@) bytes"

# =========================
# Boot image (installer disk)
# =========================
.PHONY: boot-img
boot-img: $(BOOT_IMG)

$(BOOT_IMG): $(STAGE1_BIN) $(STAGE2_BIN) $(INSTALLER_BIN) | $(OUT)
	@echo "  [boot.img] Allocating $(BOOT_IMG_SIZE_MB) MB..."
	dd if=/dev/zero of=$@ bs=1M count=$(BOOT_IMG_SIZE_MB) status=none

	@echo "  [boot.img] Writing stage1 (LBA 0)..."
	dd if=$(STAGE1_BIN) of=$@ bs=512 seek=0 conv=notrunc status=none

	@echo "  [boot.img] Writing stage2 (LBA 1)..."
	dd if=$(STAGE2_BIN) of=$@ bs=512 seek=1 conv=notrunc status=none

	@echo "  [boot.img] Writing installer (LBA 32)..."
	dd if=$(INSTALLER_BIN) of=$@ bs=512 seek=32 conv=notrunc

	@echo "  [boot.img] Done."

# =========================
# Target disk (empty)
# =========================
.PHONY: disk-img
disk-img: $(DISK_IMG)

$(DISK_IMG): | $(OUT)
	@echo "  [disk.img] Allocating $(DISK_IMG_SIZE_MB) MB..."
	dd if=/dev/zero of=$@ bs=1M count=$(DISK_IMG_SIZE_MB) status=none
	@echo "  [disk.img] Done."

# =========================
# Run QEMU (correct setup)
# =========================
.PHONY: run
run: $(BOOT_IMG) $(DISK_IMG)
	qemu-system-x86_64 \
	    -machine q35 \
	    -serial mon:stdio \
	    -d int,cpu_reset,guest_errors \
	    -D qemu.log \
	    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
	    -drive file=$(BOOT_IMG),format=raw,if=ide,index=0 \
	    -drive id=disk,format=raw,if=none,file=$(DISK_IMG) \
	    -device virtio-blk-pci,drive=disk \
	    -no-reboot

# =========================
# Debug run (with logs)
# =========================
.PHONY: run-debug
run-debug: $(BOOT_IMG)
	qemu-system-x86_64 \
	    -machine q35 \
	    -serial mon:stdio \
	    -d int,cpu_reset,guest_errors \
	    -D qemu.log \
	    -drive file=$(BOOT_IMG),format=raw,if=ide,index=0

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