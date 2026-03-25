SECTIONS {
    .commands : {
        __start_commands = .;
        KEEP(*(.commands))
        __stop_commands = .;
    }
} INSERT AFTER .rodata;