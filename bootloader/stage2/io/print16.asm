; 16-bit real mode serial output
; IN: AL = char
serial_putc:

    push dx
    push ax
    mov ah, al
    mov dx, 0x3F8 + 5


.wait:

    in al, dx
    test al, 0x20
    jz .wait
    mov dx, 0x3F8
    mov al, ah
    out dx, al
    pop ax
    pop dx
    ret

print_crlf:

    mov al, 13
    call serial_putc
    mov al, 10
    call serial_putc
    ret

; IN: SI = string
print:

    lodsb
    test al, al
    jz .done
    call serial_putc
    jmp print


.done:

    ret

; IN: AL = byte
print_hex8:

    push ax
    push cx
    mov cx, 2


.next:

    rol al, 4
    push ax
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jbe .print
    add al, 7


.print:

    call serial_putc
    pop ax
    loop .next
    pop cx
    pop ax
    ret
