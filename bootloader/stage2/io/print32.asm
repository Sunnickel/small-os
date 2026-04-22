; 32-bit protected mode serial output
; IN: AL = char
serial_putc32:

    push edx
    push eax
    mov ah, al
    mov edx, 0x3F8 + 5


.wait:

    in al, dx
    test al, 0x20
    jz .wait
    mov edx, 0x3F8
    mov al, ah
    out dx, al
    pop eax
    pop edx
    ret

print_crlf32:

    mov al, 13
    call serial_putc32
    mov al, 10
    call serial_putc32
    ret

; IN: ESI = string
print32:

    pusha
    mov edx, 0x3F8


.loop:

    lodsb
    test al, al
    jz .done

    push eax
    mov edx, 0x3F8 + 5


.wait:

    in al, dx
    test al, 0x20
    jz .wait
    pop eax

    mov edx, 0x3F8
    out dx, al
    jmp .loop


.done:

    popa
    ret
