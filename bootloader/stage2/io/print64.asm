; 64-bit long mode serial output
; IN: AL = char
serial_putc64:

    push rdx
    push rax
    mov ah, al
    mov edx, 0x3F8 + 5


.wait:

    in al, dx
    test al, 0x20
    jz .wait
    mov edx, 0x3F8
    mov al, ah
    out dx, al
    pop rax
    pop rdx
    ret

print_crlf64:

    mov al, 13
    call serial_putc64
    mov al, 10
    call serial_putc64
    ret

; IN: RSI = string
print64:

    push rax
    push rdx
    mov edx, 0x3F8


.loop:

    lodsb
    test al, al
    jz .done

    push rax
    mov edx, 0x3F8 + 5


.wait:

    in al, dx
    test al, 0x20
    jz .wait
    pop rax

    mov edx, 0x3F8
    out dx, al
    jmp .loop


.done:

    pop rdx
    pop rax
    ret
