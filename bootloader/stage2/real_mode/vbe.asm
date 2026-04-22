; ============================================================
; VBE FRAMEBUFFER
; ============================================================
detect_framebuffer:
    push ax
    push bx
    push cx
    push di
    push si

    mov si, msg_vbe_start
    call print
    call print_crlf

    mov ax, 0x4F01
    mov cx, 0x0118
    mov di, VBE_INFO_ADDR
    int 0x10
    cmp ax, 0x004F
    jne .fail

    mov ax, 0x4F02
    mov bx, 0x0118 | 0x4000
    int 0x10
    cmp ax, 0x004F
    jne .fail

    mov eax, [VBE_INFO_ADDR + 40]
    mov [fb_addr], eax
    mov ax, [VBE_INFO_ADDR + 16]
    mov [fb_width], ax
    mov ax, [VBE_INFO_ADDR + 18]
    mov [fb_height], ax
    mov al, [VBE_INFO_ADDR + 25]
    mov [fb_bpp], al

    movzx eax, word [fb_width]
    movzx ebx, byte [fb_bpp]
    shr ebx, 3
    mul ebx
    mov [fb_stride], eax

    mov si, msg_vbe_ok
    call print
    mov eax, [fb_addr]
    call print_hex8
    call print_crlf
    jmp .done

.fail:
    mov dword [fb_addr], 0
    mov si, msg_vbe_fail
    call print
    call print_crlf

.done:
    pop si
    pop di
    pop cx
    pop bx
    pop ax
    ret

msg_vbe_start: db "[stage2] vbe init...", 0
msg_vbe_ok:    db "[stage2] vbe ok fb=0x", 0
msg_vbe_fail:  db "[stage2] vbe failed", 0