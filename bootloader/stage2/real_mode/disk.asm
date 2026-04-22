
; ============================================================
; LOAD STAGE 3
; ============================================================
load_stage3:

    mov word [dap_s3 + 2], 16
    mov word [dap_s3 + 4], 0x0000
    mov word [dap_s3 + 6], 0x1000
    mov dword [dap_s3 + 8], 32
    mov dword [dap_s3 + 12], 0

    mov si, msg_dap
    call print
    mov ax, [dap_s3 + 2]
    call print_hex8
    mov al, ' '
    call serial_putc
    call print_crlf

    mov si, msg_at
    call print
    mov ax, [dap_s3 + 6]
    call print_hex8
    mov al, ':'
    call serial_putc
    mov ax, [dap_s3 + 4]
    call print_hex8
    call print_crlf

    mov si, msg_lba
    call print
    mov eax, [dap_s3 + 8]
    call print_hex8
    call print_crlf

    mov si, dap_s3
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jc .fail

    mov si, msg_s3_ok
    call print
    call print_crlf

    call enable_a20
    mov si, msg_a20
    call print
    call print_crlf

    jmp enter_protected_mode

.fail:

    mov si, msg_s3_fail
    call print
    mov al, ah
    call print_hex8
    call print_crlf
    jmp $

msg_loading_s3: db "[stage2] loading stage3...", 0



msg_dap:        db "[stage2] dap: cnt=0x", 0



msg_at:         db "[stage2] buf=0x", 0



msg_lba:        db "[stage2] lba=0x", 0



msg_s3_ok:      db "[stage2] s3 read ok", 0



msg_s3_fail:    db "[stage2] s3 error ah=0x", 0



msg_a20:        db "[stage2] a20 enabled", 0

dap_s3:

    db 0x10, 0
    dw 0, 0, 0
    dd 0, 0
