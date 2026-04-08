[BITS 16]
[ORG 0x8000]

; Constants
INSTALLER_LOAD_PHYS equ 0x10000
INSTALLER_LBA       equ 32
INSTALLER_SECTORS   equ 128
MMAP_BUFFER         equ 0x7000
BOOT_INFO_ADDR      equ 0x5000

start:
    ; Standardize CPU state immediately
    cli
    jmp 0x0000:.set_cs      ; Force CS to 0x0000 to match ORG 0x8000
.set_cs:
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov sp, 0x7C00          ; Stack grows down from 0x7C00

    mov [boot_drive], dl

    ; Initialize Serial Port (COM1)
    mov dx, 0x3F8 + 3
    mov al, 0x80
    out dx, al
    mov dx, 0x3F8
    mov al, 3
    out dx, al
    mov dx, 0x3F8 + 3
    mov al, 0x03
    out dx, al

    mov si, msg_start
    call print

    mov al, '1'
    call debug_char_16

    call detect_framebuffer
    call detect_memory

    ; Load Installer using the aligned DAP
    mov si, dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jc disk_error

    mov al, '2'
    call debug_char_16

    ; Enable A20
    in  al, 0x92
    or  al, 0x02
    out 0x92, al

    cli
    lgdt [gdt32_descriptor]
    mov eax, cr0
    or  eax, 1
    mov cr0, eax

    jmp 0x08:pmode_entry

; ─────────────────────────────────────────────
; 16-bit Helper Functions (Must be before BITS 32)
; ─────────────────────────────────────────────

debug_char_16:
    mov dx, 0x3F8
    out dx, al
    ret

print:
    lodsb
    or al, al
    jz .ret
    mov ah, 0x0E
    int 0x10
    jmp print
.ret:
    ret

detect_framebuffer:
    mov ax, 0x4F01
    mov cx, 0x0118
    mov di, 0x6000
    int 0x10
    cmp ax, 0x004F
    jne .no_vbe

    mov ax, 0x4F02
    mov bx, 0x0118 | 0x4000
    int 0x10

    mov eax, [0x6000 + 40]
    mov [fb_addr], eax
    mov ax, [0x6000 + 16]
    mov [fb_width], ax
    mov ax, [0x6000 + 18]
    mov [fb_height], ax
    mov al, [0x6000 + 25]
    mov [fb_bpp], al

    movzx eax, word [fb_width]
    movzx ebx, byte [fb_bpp]
    shr ebx, 3
    mul ebx
    mov [fb_stride], eax
    ret
.no_vbe:
    mov al, 'V'
    call debug_char_16
    ret

detect_memory:
    mov di, MMAP_BUFFER
    xor ebx, ebx
.loop:
    mov eax, 0xE820
    mov ecx, 24
    mov edx, 0x534D4150
    int 0x15
    jc .done
    inc word [mmap_count]
    add di, 24
    test ebx, ebx
    jnz .loop
.done:
    ret

disk_error:
    mov al, 'E'
    call debug_char_16
    cli
    hlt

; ─────────────────────────────────────────────
; 32-bit Protected Mode
; ─────────────────────────────────────────────
[BITS 32]
pmode_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x8FFFF

    mov al, '3'
    mov dx, 0x3F8
    out dx, al

    ; Paging
    mov edi, 0x1000
    mov cr3, edi
    xor eax, eax
    mov ecx, 1024 * 4
    rep stosd

    mov dword [0x1000], 0x2000 | 3
    mov dword [0x2000], 0x3000 | 3
    mov dword [0x3000], 0x0000 | 0x83

    ; Map FB
    mov eax, [fb_addr]
    test eax, eax
    jz .skip_fb
    mov ebx, eax
    shr ebx, 21
    shl ebx, 3
    add ebx, 0x3000
    mov edx, eax
    and edx, 0xFFE00000
    or  edx, 0x83
    mov [ebx], edx
.skip_fb:

    mov eax, cr4
    or  eax, 1 << 5
    mov cr4, eax

    mov ecx, 0xC0000080
    rdmsr
    or  eax, 1 << 8
    wrmsr

    mov eax, cr0
    or  eax, 0x80000000
    mov cr0, eax

    lidt [null_idt_descriptor]
    lgdt [gdt64_descriptor]

    mov al, '4'
    mov dx, 0x3F8
    out dx, al
    jmp 0x08:lmode_entry

; ─────────────────────────────────────────────
; 64-bit Long Mode
; ─────────────────────────────────────────────
[BITS 64]
lmode_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x8FFFF

    ; Build BootInfo
    mov rdi, BOOT_INFO_ADDR
    xor rax, rax
    mov rcx, 11
    rep stosq

    mov rdi, BOOT_INFO_ADDR
    mov qword [rdi + 0],  0
    mov qword [rdi + 8],  MMAP_BUFFER
    movzx rax, word [mmap_count]
    mov qword [rdi + 16], rax

    mov eax, [fb_addr]
    mov qword [rdi + 24], rax

    movzx rax, word [fb_width]
    movzx rbx, word [fb_height]
    mul rbx
    movzx rbx, byte [fb_bpp]
    shr rbx, 3
    mul rbx
    mov qword [rdi + 32], rax

    movzx rax, word [fb_width]
    mov qword [rdi + 40], rax
    movzx rax, word [fb_height]
    mov qword [rdi + 48], rax
    mov eax, [fb_stride]
    mov qword [rdi + 56], rax
    movzx rax, byte [fb_bpp]
    shr rax, 3
    mov qword [rdi + 64], rax
    mov qword [rdi + 72], 1

    mov qword [rdi + 80], 0

    mov rdi, BOOT_INFO_ADDR
    mov rax, INSTALLER_LOAD_PHYS
    call rax

.hang:
    hlt
    jmp .hang

; ─────────────────────────────────────────────
; Data and Descriptors
; ─────────────────────────────────────────────

align 4
dap:
    db 0x10, 0
    dw INSTALLER_SECTORS
    dw 0x0000
    dw INSTALLER_LOAD_PHYS >> 4
    dq INSTALLER_LBA

align 16
gdt32_start:
    dq 0
    dw 0xFFFF, 0x0000, 0x9A00, 0x00CF
    dw 0xFFFF, 0x0000, 0x9200, 0x00CF
gdt32_end:
gdt32_descriptor:
    dw gdt32_end - gdt32_start - 1
    dd gdt32_start

align 16
gdt64_start:
    dq 0
    dq 0x00209A0000000000
    dq 0x0000920000000000
gdt64_end:
gdt64_descriptor:
    dw gdt64_end - gdt64_start - 1
    dd gdt64_start

null_idt_descriptor:
    dw 0
    dq 0

boot_drive  db 0
fb_addr     dd 0
fb_width    dw 0
fb_height   dw 0
fb_bpp      db 0
fb_stride   dd 0
mmap_count  dw 0
msg_start   db "Booting...", 13, 10, 0