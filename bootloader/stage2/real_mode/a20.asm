enable_a20:

    in al, 0x92
    or al, 0x02
    out 0x92, al
    ret
