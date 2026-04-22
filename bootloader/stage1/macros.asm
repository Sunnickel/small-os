%macro hang 0


.hang:

    cli
    hlt
    jmp .hang
%endmacro

%macro log 1
    push si
    mov si, %1
    call serial_print
    pop si
%endmacro
