; ------------------------------------------------------------------
; SERIAL I/O
; ------------------------------------------------------------------
init_serial:

    push ax
    push dx

    mov dx, 0x3F8 + 1
    xor al, al
    out dx, al

    mov dx, 0x3F8 + 3
    mov al, 0x80
    out dx, al

    mov dx, 0x3F8 + 0
    mov al, 0x03
    out dx, al

    mov dx, 0x3F8 + 1
    xor al, al
    out dx, al

    mov dx, 0x3F8 + 3
    mov al, 0x03
    out dx, al

    mov dx, 0x3F8 + 2
    mov al, 0xC7
    out dx, al

    mov dx, 0x3F8 + 4
    mov al, 0x0B
    out dx, al

    pop dx
    pop ax
    ret

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

serial_print:

    push ax
    push si


.loop:

    lodsb
    test al, al
    jz .done
    call serial_putc
    jmp .loop


.done:

    pop si
    pop ax
    ret

; ------------------------------------------------------------------
; PRINT HELPERS
; ------------------------------------------------------------------
print_hex8:

    push ax
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
    pop ax
    ret

print_hex16:

    push ax
    mov al, ah
    call print_hex8
    pop ax
    call print_hex8
    ret

print_crlf:

    mov al, 13
    call serial_putc
    mov al, 10
    call serial_putc
    ret
