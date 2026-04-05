[BITS 16]
[ORG 0x0000]

INSTALLER_LOAD_SEG equ 0x1000  ; 0x10000 physical
INSTALLER_LBA      equ 2       ; LBA on boot disk
INSTALLER_SECTORS  equ 20

CODE_OFFSET equ 0x08
DATA_OFFSET equ 0x10

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    sti

    mov si, msg
    call print

    ; =========================
    ; Load installer kernel (LBA)
    ; =========================
    mov word [dap + 2], INSTALLER_SECTORS
    mov word [dap + 4], 0x0000
    mov word [dap + 6], INSTALLER_LOAD_SEG
    mov dword [dap + 8], INSTALLER_LBA
    mov dword [dap + 12], 0

    mov si, dap
    mov ah, 0x42
    mov dl, 0x80
    int 0x13
    jc disk_error

    ; =========================
    ; Enable A20
    ; =========================
    in  al, 0x92
    or  al, 0x02
    and al, 0xFE
    out 0x92, al

    ; =========================
    ; Enter Protected Mode
    ; =========================
    cli
    lgdt [gdt_descriptor]

    mov eax, cr0
    or  eax, 1
    mov cr0, eax

    jmp CODE_OFFSET:PModeMain

[BITS 32]
PModeMain:
    mov ax, DATA_OFFSET
    mov ds, ax
    mov es, ax
    mov ss, ax

    mov esp, 0x9C00

    ; jump to installer kernel
    jmp CODE_OFFSET:0x10000

[BITS 16]
print:
    lodsb
    or al, al
    jz .done
    mov ah, 0x0E
    int 0x10
    jmp print
.done:
    ret

disk_error:
    cli
.hang:
    hlt
    jmp .hang

; =========================
; GDT
; =========================
gdt_start:
    dq 0x0
    ; Code segment
    dw 0xFFFF
    dw 0x0000
    db 0x00
    db 10011010b
    db 11001111b
    db 0x00
    ; Data segment
    dw 0xFFFF
    dw 0x0000
    db 0x00
    db 10010010b
    db 11001111b
    db 0x00
gdt_end:

gdt_descriptor:
    dw gdt_end - gdt_start - 1
    dd gdt_start

dap:
    db 0x10
    db 0
    dw 0
    dw 0
    dw 0
    dq 0

msg db "Stage2 loaded!",0