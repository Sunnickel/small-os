[BITS 16]
[ORG 0x7C00]

; ------------------------------------------------------------------
; Entry point - CS:IP normalization first!
; ------------------------------------------------------------------
start:

    jmp 0x0000:.flush_cs


.flush_cs:

    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    mov [boot_drive], dl

    sti
    call init_serial
    mov si, msg_start
    call serial_print

    call check_lba_support

    call load_stage2
    mov si, msg_load
    call serial_print

    call print_crlf


    jmp STAGE2_SEG:STAGE2_OFF

; ------------------------------------------------------------------
; DATA
; ------------------------------------------------------------------
boot_drive:  db 0

msg_start: db "[stage1] starting...", 13, 10, 0



msg_load: db "[stage1] loading next stage", 13, 10, 0

; ------------------------------------------------------------------
; Modules (all code/data that must fit in 512 bytes)
; ------------------------------------------------------------------
%include "bootloader/stage1/macros.asm"
%include "bootloader/stage1/disk.asm"
%include "bootloader/stage1/serial.asm"

; ------------------------------------------------------------------
; BOOT SIGNATURE - MUST BE LAST 2 BYTES OF 512-BYTE SECTOR
; ------------------------------------------------------------------
times 510-($-$$) db 0
dw 0xAA55
