[BITS 16]
[ORG 0x8000]

%include "bootloader/stage2/constants.asm"

; ============================================================
; ENTRY POINT — MUST BE FIRST CODE AT 0x8000
; ============================================================
start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    mov [boot_drive], dl

    mov si, msg_start
    call print

    call detect_memory
    call detect_framebuffer
    call detect_rsdp
    jmp load_stage3


; --- 16-bit includes (BITS 16 active) ---
%include "bootloader/stage2/io/print16.asm"
%include "bootloader/stage2/real_mode/memory.asm"
%include "bootloader/stage2/real_mode/acpi.asm"
%include "bootloader/stage2/real_mode/vbe.asm"
%include "bootloader/stage2/real_mode/disk.asm"
%include "bootloader/stage2/real_mode/a20.asm"


; ============================================================
; PROTECTED MODE
; ============================================================
enter_protected_mode:
    cli
    lgdt [gdt32_desc]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pmode_entry


; ============================================================
; 32-BIT CODE
; ============================================================
[BITS 32]

%include "bootloader/stage2/io/print32.asm"
pmode_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x90000

    mov si, msg_prot
    call print32
    call print_crlf32

    call setup_paging
    call enter_long_mode
    jmp $



%include "bootloader/stage2/protected_mode/paging.asm"

enter_long_mode:
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    wrmsr

    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax

    lgdt [gdt64_desc]
    jmp 0x08:lmode_entry

; ============================================================
; 64-BIT CODE
; ============================================================
[BITS 64]

%include "bootloader/stage2/io/print64.asm"

lmode_entry:
    cld
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x90000

    mov rsi, msg_long
    call print64
    call print_crlf64

    mov rsi, 0x10000
    mov rdi, STAGE3_ADDR
    mov rcx, 65536
    rep movsb

    mov rdi, BOOT_INFO_ADDR
    xor rax, rax
    mov rcx, 16
    rep stosq

    mov rdi, BOOT_INFO_ADDR
    mov qword [rdi +  0], 0
    mov qword [rdi +  8], MMAP_BUFFER
    movzx rax, word [mmap_count]
    mov qword [rdi + 16], rax
    mov eax, [fb_addr]
    mov qword [rdi + 24], rax

    mov eax, [fb_stride]
    movzx rbx, word [fb_height]
    imul rax, rbx
    mov qword [rdi + 32], rax

    movzx rax, word [fb_width]
    mov qword [rdi + 40], rax
    movzx rax, word [fb_height]
    mov qword [rdi + 48], rax
    mov eax, [fb_stride]
    mov qword [rdi + 56], rax
    movzx rax, byte [fb_bpp]
    shr rax, 3
    mov qword [rdi + 64], rax
    mov qword [rdi + 72], 0
    mov eax, [rsdp_addr]
    mov qword [rdi + 80], rax

    mov rdi, BOOT_INFO_ADDR
    mov rax, STAGE3_ADDR
    jmp rax



.hang:
    hlt
    jmp .hang

; ============================================================
; GDTs
; ============================================================
align 16
gdt32:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF

gdt32_desc:
    dw $ - gdt32 - 1
    dd gdt32

align 16
gdt64:
    dq 0
    dq 0x00209A0000000000
    dq 0x0000920000000000

gdt64_desc:
    dw $ - gdt64 - 1
    dq gdt64

; ============================================================
; DATA
; ============================================================
msg_start: db "[stage2] starting...", 13, 10, 0
msg_prot: db "[stage2] entered protected mode", 0
msg_long: db "[stage2] entered long mode", 13, 10, 0

boot_drive:  db 0
mmap_count:  dw 0
rsdp_addr:   dd 0

fb_addr:     dd 0
fb_width:    dw 0
fb_height:   dw 0
fb_bpp:      db 0
fb_stride:   dd 0