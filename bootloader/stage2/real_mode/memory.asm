detect_memory:
    push es
    push ax
    push bx
    push cx
    push dx
    push di

    mov si, msg_mem_start
    call print
    call print_crlf

    xor ax, ax
    mov es, ax

    mov di, MMAP_BUFFER
    xor ebx, ebx
    xor bp, bp

.loop:
    cmp bp, 32
    jae .done

    mov eax, 0xE820
    mov ecx, 24
    mov edx, 0x534D4150
    int 0x15
    jc .done

    cmp eax, 0x534D4150
    jne .done

    inc bp
    add di, 24

    test ebx, ebx
    jnz .loop

.done:
    mov [mmap_count], bp

    mov si, msg_mem_done
    call print
    mov ax, bp
    call print_hex8
    call print_crlf

    pop di
    pop dx
    pop cx
    pop bx
    pop ax
    pop es
    ret

msg_mem_start: db "[stage2] scanning memory...", 0
msg_mem_done:  db "[stage2] memory entries=0x", 0