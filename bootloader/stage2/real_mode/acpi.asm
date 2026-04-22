detect_rsdp:
    push es
    push eax
    push ebx
    push ecx
    push edi

    mov si, msg_acpi_start
    call print
    call print_crlf

    xor ax, ax
    mov es, ax

    ; EBDA first
    movzx edi, word [0x40E]
    shl edi, 4
    mov ecx, 1024

.scan_ebda:
    call check_rsdp
    jc .found
    add edi, 16
    loop .scan_ebda

    ; BIOS area
    mov edi, 0xE0000
    mov ecx, 8192

.scan_bios:
    call check_rsdp
    jc .found
    add edi, 16
    loop .scan_bios

.not_found:
    mov dword [rsdp_addr], 0
    mov si, msg_acpi_fail
    call print
    call print_crlf
    jmp .done

.found:
    mov [rsdp_addr], edi
    mov si, msg_acpi_ok
    call print
    mov eax, edi
    call print_hex8
    call print_crlf

.done:
    pop edi
    pop ecx
    pop ebx
    pop eax
    pop es
    ret

check_rsdp:
    push eax
    mov eax, [edi]
    cmp eax, 0x20445352
    jne .no
    mov eax, [edi+4]
    cmp eax, 0x20525450
    jne .no
    pop eax
    stc
    ret
.no:
    pop eax
    clc
    ret

msg_acpi_start: db "[stage2] scanning acpi...", 0
msg_acpi_ok:    db "[stage2] acpi rsdp=0x", 0
msg_acpi_fail:  db "[stage2] acpi not found", 0
