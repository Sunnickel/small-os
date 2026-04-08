ENTRY(_start)
OUTPUT_FORMAT(elf64-x86-64)

SECTIONS {
    . = 0x10000;
    .text :
        {
            KEEP(*(.text.entry))
            *(.text .text.*)
        }

    .rodata : { *(.rodata .rodata.*) }

    .commands : {
        __start_commands = .;
        KEEP(*(.commands))
        __stop_commands = .;
    }

    .data : { *(.data .data.*) }

    .bss :
    {
        __bss_start = .;
        *(.bss .bss.*)
        __bss_end = .;
    }

    /DISCARD/ :
    {
        *(.comment)
        *(.eh_frame)
        *(.rel.plt)
    }
}