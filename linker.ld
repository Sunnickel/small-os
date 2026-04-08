ENTRY(_start)

SECTIONS {
    . = 0x10000;

    .text : {
        *(.text.entry)   /* put _start first if you mark it */
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .commands : {
        __start_commands = .;
        KEEP(*(.commands))
        __stop_commands = .;
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.note .note.*)
    }
}