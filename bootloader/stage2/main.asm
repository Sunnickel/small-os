[BITS 16]
[ORG 0x8000]

; ─────────────────────────────────────────────
; CONSTANTS
; ─────────────────────────────────────────────
MMAP_BUFFER     equ 0xF000   ; E820 entries stored here (max ~21 entries before 0xFC00)
BOOT_INFO_ADDR  equ 0xFF00   ; BootInfo struct passed to stage3
STAGE3_ADDR     equ 0x600000 ; Above the heap hole (0x2000000-0x2FFFFFF)
VBE_INFO_ADDR   equ 0xFC00   ; VBE mode info block (moved up, away from E820 buffer)

start:
	cli

	; ─────────────────────────────
	; REAL MODE SETUP
	; ─────────────────────────────
	xor ax, ax
	mov ds, ax
	mov es, ax
	mov ss, ax
	mov sp, 0x7C00

	mov [boot_drive], dl

	call detect_memory
	call detect_framebuffer

; ============================================================
; LOAD STAGE 3
; Load temporarily at 0x10000 (64 KB mark), copy up after
; paging is enabled and we're in long mode.
; ============================================================
load_stage3:
	mov word  [dap_s3 + 2],  16       ; sector count
	mov word  [dap_s3 + 4],  0x0000   ; buffer offset
	mov word  [dap_s3 + 6],  0x1000   ; buffer segment  → 0x10000
	mov dword [dap_s3 + 8],  32       ; LBA low
	mov dword [dap_s3 + 12], 0        ; LBA high

	mov si, dap_s3
	mov ah, 0x42
	mov dl, [boot_drive]
	int 0x13
	jc .fail

	mov si, s3loaded
	call print
	call enable_a20
	jmp enter_protected_mode

.fail:
	mov si, s3err
	call print
	mov al, ah
	call print_hex8
	jmp $

; ─── helpers ────────────────────────────────────────────────
print_hex8:
	push ax
	mov cx, 2
.next:
	rol al, 4
	push ax
	and al, 0x0F
	add al, '0'
	cmp al, '9'
	jbe .print
	add al, 7
.print:
	mov ah, 0x0E
	int 0x10
	pop ax
	loop .next
	pop ax
	ret

s3loaded db "Stage3 loaded", 0
s3err    db "Stage3 load error: ", 0

dap_s3:
	db 0x10, 0   ; size, reserved
	dw 0         ; sector count  (filled above)
	dw 0         ; buffer offset (filled above)
	dw 0         ; buffer segment(filled above)
	dd 0         ; LBA low       (filled above)
	dd 0         ; LBA high      (filled above)

; ============================================================
; REAL MODE: MEMORY DETECTION (E820)
; ============================================================
detect_memory:
	push es
	xor ax, ax
	mov es, ax

	mov di, MMAP_BUFFER
	xor ebx, ebx
	xor bp, bp

.loop:
	mov eax, 0xE820
	mov ecx, 24
	mov edx, 0x534D4150
	int 0x15
	jc .done

	cmp eax, 0x534D4150
	jne .done

	add di, 24
	inc bp

	test ebx, ebx
	jnz .loop

.done:
	mov [mmap_count], bp
	pop es
	ret

; ============================================================
; REAL MODE: VBE FRAMEBUFFER
; ============================================================
detect_framebuffer:
	; Query mode 0x0118 (1024×768 24-bpp)
	mov ax, 0x4F01
	mov cx, 0x0118
	mov di, VBE_INFO_ADDR
	int 0x10
	cmp ax, 0x004F
	jne .fail

	; Activate the mode (bit 14 = use linear framebuffer)
	mov ax, 0x4F02
	mov bx, 0x0118 | 0x4000
	int 0x10

	; Pull fields from the VBE ModeInfo block
	mov eax, [VBE_INFO_ADDR + 40]   ; PhysBasePtr
	mov [fb_addr], eax
	mov ax,  [VBE_INFO_ADDR + 16]   ; XResolution
	mov [fb_width], ax
	mov ax,  [VBE_INFO_ADDR + 18]   ; YResolution
	mov [fb_height], ax
	mov al,  [VBE_INFO_ADDR + 25]   ; BitsPerPixel
	mov [fb_bpp], al

	; stride = width * (bpp / 8)
	movzx eax, word [fb_width]
	movzx ebx, byte [fb_bpp]
	shr ebx, 3                       ; bytes per pixel
	mul ebx                          ; eax = stride (≤ 64 KB, no overflow)
	mov [fb_stride], eax
	ret

.fail:
	ret

; ============================================================
; ENABLE A20 (fast gate)
; ============================================================
enable_a20:
	in  al, 0x92
	or  al, 0x02
	out 0x92, al
	ret

; ============================================================
; ENTER PROTECTED MODE
; ============================================================
enter_protected_mode:
	cli
	lgdt [gdt32_desc]
	mov eax, cr0
	or  eax, 1
	mov cr0, eax
	jmp 0x08:pmode_entry

; ============================================================
; PROTECTED MODE
; ============================================================
[BITS 32]
pmode_entry:
	mov ax, 0x10
	mov ds, ax
	mov es, ax
	mov ss, ax
	mov esp, 0x90000

	call setup_paging
	call enter_long_mode
	jmp $

; ============================================================
; PAGING
;
; Identity-maps all 4 GB with 2 MB huge pages, then carves
; out 8 empty 4 KB page tables for the heap region
; 0x2000000–0x2FFFFFF (PD[0] entries 16–23).
;
; Physical layout:
;   0x1000          PML4  (1 page)
;   0x2000          PDPT  (1 page)
;   0x3000          PD[0]  0–1 GB     (PDPT[0])
;   0x4000          PD[1]  1–2 GB     (PDPT[1])
;   0x5000          PD[2]  2–3 GB     (PDPT[2])
;   0x6000          PD[3]  3–4 GB     (PDPT[3])
;   0x7000–0xE000   8 empty PTs for heap (pointed to by PD[0][16..23])
;
; Total: 15 pages → clear 0x1000–0xEFFF (15 × 4096 bytes).
;
; NOTE: STAGE3_ADDR = 0x400000 lives at PD[0] entry 2, which
; is a normal huge-page entry well outside the heap hole.
; ============================================================
setup_paging:
    ; ── Clear pages 0x70000–0x7EFFF (16 pages now, +1 for high PDPT) ──
    mov edi, 0x70000
    xor eax, eax
    mov ecx, 16 * 1024          ; 16 pages
    rep stosd

    ; ── PML4[0] → PDPT @ 0x71000  (0–512GB, existing) ──────────────────
    mov dword [0x70000 + 0*8],     0x71003
    mov dword [0x70000 + 0*8 + 4], 0

    ; ── PML4[1] → PDPT @ 0x7E000  (512GB–1TB, NEW for high MMIO) ───────
    mov dword [0x70000 + 1*8],     0x7E003
    mov dword [0x70000 + 1*8 + 4], 0

    ; ── PDPT[0..3] → PDs @ 0x72000–0x75000  (existing, 0–4GB) ──────────
    mov dword [0x71000 + 0*8],     0x72003
    mov dword [0x71000 + 0*8 + 4], 0
    mov dword [0x71000 + 1*8],     0x73003
    mov dword [0x71000 + 1*8 + 4], 0
    mov dword [0x71000 + 2*8],     0x74003
    mov dword [0x71000 + 2*8 + 4], 0
    mov dword [0x71000 + 3*8],     0x75003
    mov dword [0x71000 + 3*8 + 4], 0

    ; ── Fill PDs 0x72000–0x75000 with 2MB huge pages (existing) ─────────
    mov edi, 0x72000
    mov eax, 0x83
    mov ecx, 2048
.pd_loop:
    mov  [edi], eax
    mov  dword [edi+4], 0
    add  eax, 0x200000
    add  edi, 8
    loop .pd_loop

    ; ── Heap hole PD[0][16..23] (existing) ──────────────────────────────
    mov ecx, 8
    mov edi, 0x72000 + 16*8
    mov eax, 0x76003
.heap_pts:
    mov  [edi], eax
    mov  dword [edi+4], 0
    add  eax, 0x1000
    add  edi, 8
    loop .heap_pts

    mov edi, 0x7E000
    mov ecx, 512
    mov eax, 0

    mov eax, 0x83              ; flags: present + writable + PS (1GB huge)
    mov edx, 0x80              ; high dword: PA starts at 512GB = 0x80_xxxxxxxx
.high_pdpt_loop:
    mov  [edi],   eax          ; low dword: PA[31:0] | flags
    mov  [edi+4], edx          ; high dword: PA[63:32]
    add  eax, 0x40000000       ; next 1GB (add to low dword)
    jnc  .no_carry
    inc  edx                   ; carry into high dword
.no_carry:
    add  edi, 8
    loop .high_pdpt_loop

    ; ── Load CR3 ─────────────────────────────────────────────────────────
    mov eax, 0x70000
    mov cr3, eax
    ret

; ============================================================
; ENTER LONG MODE
; ============================================================
enter_long_mode:
	; Enable PAE (CR4.PAE)
	mov eax, cr4
    or  eax, (1 << 5)
    mov cr4, eax

	; Set EFER.LME
	mov ecx, 0xC0000080
	rdmsr
	or  eax, (1 << 8)
	wrmsr

	; Enable paging (CR0.PG) — activates long mode
	mov eax, cr0
	or  eax, 0x80000000
	mov cr0, eax

	lgdt [gdt64_desc]
	jmp 0x08:lmode_entry

; ============================================================
; LONG MODE ENTRY
; ============================================================
[BITS 64]
lmode_entry:
	cld
	mov ax, 0x10
	mov ds, ax
	mov es, ax
	mov ss, ax
	mov rsp, 0x90000

	; ── Copy stage3 from 0x10000 → STAGE3_ADDR (0x400000) ──
	mov rsi, 0x10000
	mov rdi, STAGE3_ADDR
	mov rcx, 65536              ; 16 sectors × 4096 = 64 KB
	rep movsb

	; ── Build BootInfo at BOOT_INFO_ADDR (0xFF00) ───────────
	;
	; Rust BootInfo layout (all fields u64, packed/repr(C)):
	;   +0   physical_memory_offset
	;   +8   memory_map              (physical addr of E820 buffer)
	;   +16  memory_map_len
	;   +24  fb_addr                 ─┐
	;   +32  fb_size                  │
	;   +40  fb_width                 │ FrameBufferInfo
	;   +48  fb_height                │
	;   +56  fb_stride                │
	;   +64  fb_bytes_per_pixel       │
	;   +72  fb_pixel_format         ─┘
	;   +80  rsdp_addr
	;   +88  fat32_partition_lba     (filled by stage3)
	;   +96  boot_disk               (filled by stage3)
	; Total: 104 bytes — fits in the 256-byte gap before 0x10000.

	; Zero 128 bytes (16 qwords) at BOOT_INFO_ADDR
	mov rdi, BOOT_INFO_ADDR
	xor rax, rax
	mov rcx, 16
	rep stosq

	mov rdi, BOOT_INFO_ADDR

	; +0  physical_memory_offset = 0  (identity map: virt == phys)
	mov qword [rdi +  0], 0

	; +8  memory_map physical address
	mov qword [rdi +  8], MMAP_BUFFER

	; +16 memory_map_len (number of E820 entries)
	movzx rax, word [mmap_count]
	mov qword [rdi + 16], rax

	; +24 fb_addr  (zero-extend 32-bit physical address)
	mov eax, [fb_addr]
	mov qword [rdi + 24], rax

	; +32 fb_size = stride * height
	mov eax, [fb_stride]        ; 32-bit → zero-extends into rax
	movzx rbx, word [fb_height]
	imul rax, rbx               ; FIX: use imul (no RDX clobber, safe 64-bit)
	mov qword [rdi + 32], rax

	; +40 fb_width
	movzx rax, word [fb_width]
	mov qword [rdi + 40], rax

	; +48 fb_height
	movzx rax, word [fb_height]
	mov qword [rdi + 48], rax

	; +56 fb_stride
	mov eax, [fb_stride]
	mov qword [rdi + 56], rax

	; +64 fb_bytes_per_pixel = bpp / 8
	movzx rax, byte [fb_bpp]
	shr rax, 3
	mov qword [rdi + 64], rax

	; +72 fb_pixel_format = 0 (BGR/RGB — stage3 knows)
	mov qword [rdi + 72], 0

	; +80 rsdp_addr = 0 (stage3 scans ACPI tables itself)
	mov qword [rdi + 80], 0

	; +88, +96  left zero — stage3 fills fat32_partition_lba / boot_disk

	; ── Jump to stage3 with BootInfo pointer in RDI ─────────
	mov rdi, BOOT_INFO_ADDR
	mov rax, STAGE3_ADDR
	jmp rax

.hang:
	hlt
	jmp .hang

; ============================================================
; GDTs
; ============================================================
align 16
gdt32:
	dq 0                       ; null
	dq 0x00CF9A000000FFFF      ; 32-bit code, ring 0
	dq 0x00CF92000000FFFF      ; 32-bit data, ring 0

gdt32_desc:
	dw $ - gdt32 - 1
	dd gdt32

align 16
gdt64:
	dq 0                       ; null
	dq 0x00209A0000000000      ; 64-bit code, ring 0  (L=1, D=0)
	dq 0x0000920000000000      ; 64-bit data, ring 0

gdt64_desc:
	dw $ - gdt64 - 1
	dq gdt64

; ── 16-bit print (BIOS teletype) ────────────────────────────
print:
	lodsb
	or  al, al
	jz  .done
	mov ah, 0x0E
	int 0x10
	jmp print
.done:
	ret

; ============================================================
; DATA
; ============================================================
boot_drive  db 0
mmap_count  dw 0

fb_addr     dd 0
fb_width    dw 0
fb_height   dw 0
fb_bpp      db 0
fb_stride   dd 0