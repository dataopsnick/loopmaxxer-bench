# =============================================================================
# Open960 (RISC80960) - Boot & Self-Test Assembly Program
# =============================================================================

.text
_start:
    # 1. Initialize Stack Pointer and Frame Pointer
    lda     0x4000, sp          # SP = 0x4000
    lda     0x3000, fp          # FP = 0x3000
    lda     0xFF000000, g0      # g0 = UART Base Address (0xFF000000)

    # 2. Print 'O', 'P', 'E', 'N', '9', '6', '0', '\n' to UART
    lda     0x4F, g1            # 'O'
    stob    g1, (g0)
    lda     0x50, g1            # 'P'
    stob    g1, (g0)
    lda     0x45, g1            # 'E'
    stob    g1, (g0)
    lda     0x4E, g1            # 'N'
    stob    g1, (g0)
    lda     0x39, g1            # '9'
    stob    g1, (g0)
    lda     0x36, g1            # '6'
    stob    g1, (g0)
    lda     0x30, g1            # '0'
    stob    g1, (g0)
    lda     0x0A, g1            # '\n'
    stob    g1, (g0)

    # 3. Arithmetic Verification
    lda     100, g2
    lda     250, g3
    addo    g2, g3, g4          # g4 = 100 + 250 = 350
    cmpibe  350, g4, test_sub   # Verify g4 == 350

test_fail:
    lda     0xFF0000F0, g10
    lda     0xDEADBEEF, g11
    st      g11, (g10)          # Signal failure
halt_loop:
    b       halt_loop

test_sub:
    subo    g2, g4, g5          # g5 = 350 - 100 = 250
    cmpibe  250, g5, test_logic
    b       test_fail

test_logic:
    lda     0x0F, g6
    lda     0xF0, g7
    or      g6, g7, g8          # g8 = 0xFF
    cmpibe  255, g8, test_pass
    b       test_fail

test_pass:
    # Print 'P', 'A', 'S', 'S', '\n'
    lda     0x50, g1            # 'P'
    stob    g1, (g0)
    lda     0x41, g1            # 'A'
    stob    g1, (g0)
    lda     0x53, g1            # 'S'
    stob    g1, (g0)
    lda     0x53, g1            # 'S'
    stob    g1, (g0)
    lda     0x0A, g1            # '\n'
    stob    g1, (g0)

    # 4. Exit simulation with SUCCESS (0)
    lda     0xFF0000F0, g10
    lda     0x00000000, g11
    st      g11, (g10)

success_halt:
    b       success_halt
