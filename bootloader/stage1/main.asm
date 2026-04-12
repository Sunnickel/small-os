[BITS 16]
[ORG 0x7C00]

STAGE2_SEG     equ 0x0800
STAGE2_SECTORS equ 10

start:
	cli
	xor ax, ax
	mov ds, ax
	mov ss, ax
	mov sp, 0x7A00
	sti

	; Save boot drive from BIOS
	mov [boot_drive], dl

	; =========================
	; Check INT13 Extensions
	; =========================
	mov ah, 0x41
	mov bx, 0x55AA
	mov dl, [boot_drive]
	int 0x13
	jc disk_error
	cmp bx, 0xAA55
	jne disk_error

	; =========================
	; Load stage2 (LBA)
	; =========================
	mov word [dap + 2], STAGE2_SECTORS
	mov word [dap + 4], 0x0000
	mov word [dap + 6], STAGE2_SEG
	mov dword [dap + 8], 1
	mov dword [dap + 12], 0

	mov si, dap
	mov ah, 0x42
	mov dl, [boot_drive]
	int 0x13
	jc disk_error

	mov si, stgld
	call print

	; Jump to stage2
	jmp STAGE2_SEG:0x0000

disk_error:
	mov si, err_msg
	call print
	cli
.hang:
	hlt
	jmp .hang

print:
	lodsb
	or al, al
	jz .done
	mov ah, 0x0E
	int 0x10
	jmp print
.done:
	ret

boot_drive db 0
err_msg db "Stage1 disk error!",0
stgld db "Stage2 loaded",0

; =========================
; Disk Address Packet
; =========================
dap:
	db 0x10
	db 0
	dw 0
	dw 0
	dw 0
	dq 0

times 510-($-$$) db 0
dw 0xAA55