STAGE2_SEG     equ 0x0800
STAGE2_OFF     equ 0x0000
STAGE2_SECTORS equ 10

; ------------------------------------------------------------------
; LBA SUPPORT CHECK
; ------------------------------------------------------------------
check_lba_support:

    mov ah, 0x41
    mov bx, 0x55AA
    mov dl, [boot_drive]
    int 0x13
    jc disk_error
    cmp bx, 0xAA55
    jne disk_error
    test cx, 1
    jz disk_error
    ret

load_stage2:

    xor ax, ax
    mov es, ax
    mov word [dap + 2], STAGE2_SECTORS
    mov word [dap + 4], STAGE2_OFF
    mov word [dap + 6], STAGE2_SEG
    mov dword [dap + 8], 1
    mov dword [dap + 12], 0
    mov si, dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jc disk_error
    ret

disk_error:

    mov al, 'E'                            ; 'E' = Error
    call serial_putc
    mov al, ah                             ; Print error code in hex
    call print_hex8
    cli
    hlt
    jmp $


; ── CONSTANTS ───────────────────────────────────────────────
STAGE2_SEG     equ 0x0800
STAGE2_OFF     equ 0x0000
STAGE2_SECTORS equ 10

dap:

    db 0x10, 0
    dw 0, 0, 0
    dq 0

