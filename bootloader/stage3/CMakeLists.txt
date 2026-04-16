cmake_minimum_required(VERSION 3.20)

# ── Toolchain (set before project()) ─────────────────────────────────────────
set(CMAKE_TOOLCHAIN_FILE "${CMAKE_SOURCE_DIR}/toolchain-x86_64-elf.cmake"
        CACHE FILEPATH "Toolchain file")

project(stage3 LANGUAGES C ASM_NASM)

# ── Sources ───────────────────────────────────────────────────────────────────
set(STAGE3_SOURCES
        src/entry.asm
        src/main.c
        src/fat32.c
        src/elf_loader.c
        src/debug.c
        src/disk_probe.c
        src/virtio_blk.c
        src/libc.c
)

add_executable(stage3.elf ${STAGE3_SOURCES})

target_include_directories(stage3.elf PRIVATE include)

# ── Compiler flags ────────────────────────────────────────────────────────────
target_compile_options(stage3.elf PRIVATE
        -ffreestanding
        -fno-stack-protector
        -fno-pic
        -fno-pie
        -mno-red-zone
        -mno-mmx
        -mno-sse
        -mno-sse2
        -Wall
        -Wextra
        -O2
        -std=c11
)

# ── Linker flags ──────────────────────────────────────────────────────────────
target_link_options(stage3.elf PRIVATE
        -T ${CMAKE_SOURCE_DIR}/linker.ld
        -nostdlib
        -static
        -Wl,--build-id=none
)

set_target_properties(stage3.elf PROPERTIES
        LINK_DEPENDS "${CMAKE_SOURCE_DIR}/linker.ld"
)

# ── Post-build: strip to flat binary ─────────────────────────────────────────
add_custom_command(TARGET stage3.elf POST_BUILD
        COMMAND x86_64-elf-objcopy -O binary
        $<TARGET_FILE:stage3.elf>
        ${CMAKE_BINARY_DIR}/stage3.bin
        COMMENT "Stripping stage3.elf → stage3.bin"
)