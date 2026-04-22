bits 64

extern stage3_main

section .text.entry
global _start

_start:

    mov rsp, 0x1F0000    ; Set up a private stack (1MB region below 0x200000 is safe)

    call stage3_main     ; Call C entry point

.hang:

    hlt
    jmp .hang
