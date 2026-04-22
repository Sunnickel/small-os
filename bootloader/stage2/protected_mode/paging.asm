setup_paging:

    mov edi, 0x70000
    xor eax, eax
    mov ecx, 16 * 1024
    rep stosd

    mov dword [0x70000 + 0*8],     0x71003
    mov dword [0x70000 + 0*8 + 4], 0
    mov dword [0x70000 + 1*8],     0x7E003
    mov dword [0x70000 + 1*8 + 4], 0

    mov dword [0x71000 + 0*8],     0x72003
    mov dword [0x71000 + 0*8 + 4], 0
    mov dword [0x71000 + 1*8],     0x73003
    mov dword [0x71000 + 1*8 + 4], 0
    mov dword [0x71000 + 2*8],     0x74003
    mov dword [0x71000 + 2*8 + 4], 0
    mov dword [0x71000 + 3*8],     0x75003
    mov dword [0x71000 + 3*8 + 4], 0

    mov edi, 0x72000
    mov eax, 0x83
    mov ecx, 2048


.pd_loop:

    mov [edi], eax
    mov dword [edi+4], 0
    add eax, 0x200000
    add edi, 8
    loop .pd_loop

    mov ecx, 8
    mov edi, 0x72000 + (16*8)
    mov eax, 0x76003


.heap_pts:

    mov [edi], eax
    mov dword [edi+4], 0
    add eax, 0x1000
    add edi, 8
    loop .heap_pts

    mov edi, 0x7E000
    mov ecx, 512
    mov eax, 0
    mov edx, 0x80


.high_pdpt_loop:

    mov [edi], eax
    mov [edi+4], edx
    add eax, 0x40000000
    jnc .no_carry
    inc edx


.no_carry:

    add edi, 8
    loop .high_pdpt_loop

    mov eax, 0x70000
    mov cr3, eax
    ret
