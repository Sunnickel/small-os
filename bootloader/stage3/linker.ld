ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : {
            *(.text.entry)
            *(.text*)
        }
    .rodata : { *(.rodata*) }
    .data : { *(.data*) }
    .bss : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : { *(.comment) *(.eh_frame) }
}