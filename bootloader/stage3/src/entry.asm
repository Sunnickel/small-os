; entry.asm — Stage 3 entry trampoline
;
; Stage 2 does:
;   mov rdi, BOOT_INFO_ADDR   ; (0xFF00)
;   jmp STAGE3_ADDR           ; (0x200000)
;
; We land here in 64-bit long mode.  RDI already holds the BootInfo pointer
; per the System-V AMD64 calling convention, so we just need a stack and
; a cleared BSS before calling into C.

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