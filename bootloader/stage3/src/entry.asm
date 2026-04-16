bits 64

extern stage3_main

section .text.entry
global _start

_start:
    ; Set up a private stack (1MB region below 0x200000 is safe)
    mov rsp, 0x1F0000
    ; Call C entry point
    call stage3_main

    ; Should never return — halt
.hang:
    hlt
    jmp .hang