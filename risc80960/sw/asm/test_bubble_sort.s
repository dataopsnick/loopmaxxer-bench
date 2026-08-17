# =============================================================================
# Open960 (RISC80960) - Bubble Sort Algorithm Test
# =============================================================================

.text
_start:
    lda     0x4000, sp
    lda     0x3000, fp
    lda     0xFF000000, g0

    # 1. Initialize an array of 4 words in RAM at 0x1000: [90, 10, 40, 20]
    lda     0x1000, g1
    lda     90, g2
    st      g2, 0(g1)
    lda     10, g2
    st      g2, 4(g1)
    lda     40, g2
    st      g2, 8(g1)
    lda     20, g2
    st      g2, 12(g1)

    # 2. Bubble Sort Outer Loop (pass count = 3)
    lda     3, g3               # Outer loop counter
outer_loop:
    lda     0, g4               # Inner loop index (offset in bytes: 0, 4, 8)
inner_loop:
    addo    g1, g4, g5          # g5 = array + offset
    ld      0(g5), g6           # g6 = array[i]
    ld      4(g5), g7           # g7 = array[i+1]
    cmpibg  g6, g7, swap_elems  # If array[i] > array[i+1], swap
    b       next_inner

swap_elems:
    st      g7, 0(g5)
    st      g6, 4(g5)

next_inner:
    addo    4, g4, g4           # offset += 4
    cmpibl  g4, 12, inner_loop  # if offset < 12, continue inner loop

    subo    1, g3, g3           # outer_count -= 1
    cmpibg  g3, 0, outer_loop   # if outer_count > 0, continue outer loop

    # 3. Verify sorted array: [10, 20, 40, 90]
    ld      0(g1), g8
    cmpibe  10, g8, chk_elem1
    b       fail

chk_elem1:
    ld      4(g1), g8
    cmpibe  20, g8, chk_elem2
    b       fail

chk_elem2:
    ld      8(g1), g8
    cmpibe  40, g8, chk_elem3
    b       fail

chk_elem3:
    ld      12(g1), g8
    cmpibe  90, g8, pass
    b       fail

fail:
    lda     0xFF0000F0, g15
    lda     1, g1
    st      g1, (g15)
halt_fail:
    b       halt_fail

pass:
    # Print 'S', 'O', 'R', 'T', 'O', 'K', '\n'
    lda     0x53, g1
    stob    g1, (g0)
    lda     0x4F, g1
    stob    g1, (g0)
    lda     0x52, g1
    stob    g1, (g0)
    lda     0x54, g1
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
