# =============================================================================
# Open960 (RISC80960) - Procedure Call and Return Frame Cache Test
# =============================================================================

.text
_start:
    lda     0x4000, sp
    lda     0x3000, fp
    lda     0xFF000000, g0

    # Call procedure func_a with arguments in g1, g2
    lda     15, g1
    lda     25, g2
    call    func_a

    # After return: g3 should contain (15+25)*2 = 80
    cmpibe  80, g3, pass
    b       fail

func_a:
    # Local registers r3-r15 are scratch inside func_a
    addo    g1, g2, r3          # r3 = 40
    call    func_double
    ret

func_double:
    addo    r3, r3, g3          # g3 = 40 + 40 = 80
    ret

fail:
    lda     0xFF0000F0, g15
    lda     1, g1
    st      g1, (g15)
halt_fail:
    b       halt_fail

pass:
    # Print 'C', 'A', 'L', 'L', 'O', 'K', '\n'
    lda     0x43, g1
    stob    g1, (g0)
    lda     0x41, g1
    stob    g1, (g0)
    lda     0x4C, g1
    stob    g1, (g0)
    lda     0x4C, g1
    stob    g1, (g0)
    lda     0x4F, g1
    stob    g1, (g0)
    lda     0x4B, g1
    stob    g1, (g0)
    lda     0x0A, g1
    stob    g1, (g0)

    lda     0xFF0000F0, g15
    lda     0, g1
    st      g1, (g15)
halt_pass:
    b       halt_pass
