[BITS 16]
[ORG 0x7C00]

STAGE2_SEG     equ 0x0800      ; Where to load stage2 (16:16 = 0x08000)
STAGE2_SECTORS equ 10          ; Number of sectors for stage2
BOOT_DRIVE     equ 0x80

start:
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7C00
    sti

    mov dl, BOOT_DRIVE

    ; =========================
    ; LBA load stage2
    ; =========================
    mov word [dap + 2], STAGE2_SECTORS
    mov word [dap + 4], 0x0000
    mov word [dap + 6], STAGE2_SEG
    mov dword [dap + 8], 1          ; Stage2 starts at LBA 1
    mov dword [dap + 12], 0

    mov si, dap
    mov ah, 0x42
    int 0x13
    jc disk_error

    ; Jump to stage2
    jmp STAGE2_SEG:0x0000

disk_error:
    cli
.hang:
    hlt
    jmp .hang

; =========================
; Disk Address Packet (LBA)
; =========================
dap:
    db 0x10
    db 0
    dw 0      ; sectors
    dw 0      ; offset
    dw 0      ; segment
    dq 0      ; LBA

; =========================
; Boot signature
; =========================
times 510-($-$$) db 0
dw 0xAA55