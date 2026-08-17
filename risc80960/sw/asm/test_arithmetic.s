# =============================================================================
# Open960 (RISC80960) - Comprehensive Arithmetic & Logic Test Suite
# =============================================================================

.text
_start:
    # Set up Stack & Output
    lda     0x4000, sp
    lda     0x3000, fp
    lda     0xFF000000, g0      # UART

    # 1. Addition and Subtraction
    lda     42, g1
    lda     58, g2
    addo    g1, g2, g3          # g3 = 100
    cmpibe  100, g3, chk_sub
    b       fail

chk_sub:
    subo    g1, g3, g4          # g4 = 58
    cmpibe  58, g4, chk_shift
    b       fail

chk_shift:
    # 2. Shift Operations
    lda     1, g5
    shlo    4, g5, g6           # g6 = 16
    cmpibe  16, g6, chk_shro
    b       fail

chk_shro:
    shro    2, g6, g7           # g7 = 4
    cmpibe  4, g7, chk_bitops
    b       fail

chk_bitops:
    # 3. Bit Manipulation
    lda     0x00, g8
    setbit  5, g8, g9           # g9 = 0x20 (32)
    cmpibe  32, g9, chk_clrbit
    b       fail

chk_clrbit:
    clrbit  5, g9, g10          # g10 = 0
    cmpibe  0, g10, chk_mul
    b       fail

chk_mul:
    # 4. Multiplication & Division
    lda     12, g11
    lda     9, g12
    mulo    g11, g12, g13       # g13 = 108
    cmpibe  108, g13, chk_div
    b       fail

chk_div:
    divo    g11, g13, g14       # g14 = 9
    cmpibe  9, g14, pass
    b       fail

fail:
    lda     0xFF0000F0, g15
    lda     1, g1
    st      g1, (g15)
halt_fail:
    b       halt_fail

pass:
    # Print 'O', 'K', '\n'
    lda     0x4F, g1
    stob    g1, (g0)
    lda     0x4B, g1
    stob    g1, (g0)
    lda     0x0A, g1
    stob    g1, (g0)

    # Success Exit
    lda     0xFF0000F0, g15
    lda     0, g1
    st      g1, (g15)
halt_pass:
    b       halt_pass
