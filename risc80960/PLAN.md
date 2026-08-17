Here is the complete architectural specification, clean-room methodology, and implementation plan to build **Open960 (RISC80960)** — a fully synthesizable, cycle-accurate, open-source clean-room RTL duplicate of the Intel 80960 (i960) 32-bit RISC microprocessor with drop-in bus and pin compatibility.

---

# Open960 / RISC80960 Architectural Blueprint & Implementation Plan

```
                              +-------------------------------------------------------------+
                              |                     OPEN960 CORE                           |
                              |                                                             |
   +------------------+       |  +------------+   +------------+   +---------------------+  |       +--------------------+
   |                  |       |  |  Global    |   | Local Reg  |   | Local Frame Cache   |  |       |                    |
   |   Instruction    |       |  |  Regfile   |   | Regfile    |   | (4-8 on-chip frames)|  |       |   L-Bus / Pinout   |
   |   Fetch & PC     |------>|  |  (g0-g15)  |   | (r0-r15)   |   | Auto Spill/Fill FSM |  |------>|   BIU Interface    |
   |   (CTRL / COBR)  |       |  +------------+   +------------+   +---------------------+  |       |   (LAD[31:0],      |
   |                  |       |        |                |                     |             |       |    ADS#, READY#,   |
   +------------------+       |  +-------------------------------------------------------+  |       |    BLAST#, BE#)    |
          |                   |  |          5-Stage Pipeline / Execution Engine          |  |       |                    |
   +------------------+       |  |  [IF] -> [ID/DEC] -> [EX/ALU] -> [MEM/LSU] -> [WB]    |  |       +--------------------+
   | Instruction      |------>|  +-------------------------------------------------------+  |                 |
   | Decoder          |       |        |                        |                           |                 |
   | (REG/COBR/MEM/   |       |  +---------------+      +--------------------------------+  |                 |
   |  CTRL formats)   |       |  | 32-bit ALU    |      | Load/Store Unit (LSU)          |  |                 |
   +------------------+       |  | (16 Boolean,  |      | (Byte, Short, Word, Multi-word |  |                 |
                              |  |  Bitfield,    |      |  ldl/ldt/ldq, stl/stt/stq,     |  |                 |
                              |  |  Mul/Div, AC) |      |  MEMB scale-index addressing)  |  |                 |
                              |  +---------------+      +--------------------------------+  |                 |
                              +-------------------------------------------------------------+                 |
                                                             |                                                |
                                                             v                                                v
                                              +-------------------------------+              +--------------------------------+
                                              | Open-Source EDA (Verilator,   |              | Physical Form Factor           |
                                              | Yosys, OpenROAD, Cocotb)      |              | (132-PGA / 164-PQFP / FPGA)    |
                                              +-------------------------------+              +--------------------------------+
```

---

## 1. Clean-Room Strategy & Legal Framework

1. **IP Isolation**: All RTL (SystemVerilog/Verilog) is written from scratch based exclusively on publicly available functional architecture documentation (e.g. Intel 80960 Programmer's Reference Manual, public datasheets, and published instruction set definitions). No proprietary Intel schematics, microcode, or netlists are consulted.
2. **Instruction Set Compatibility**: Fully implements the i960 Core Architecture (KA/KB integer baseline, extensible to Jx/CA superscalar extensions).
3. **Open-Source Toolchain**: Built for synthesis with **Yosys**, simulation with **Verilator** and **Cocotb**, and physical layout / GDSII generation with **OpenROAD / OpenLane** targeting open PDKs (SkyWater 130nm / GlobalFoundries 180nm) as well as modern FPGAs (Xilinx, Lattice, Efinix).

---

## 2. Processor Architecture Specification

### 2.1 Register File Organization
The i960 register architecture provides 32 general-purpose 32-bit registers split into two sets:
1. **Global Registers (`g0` - `g15`)**:
   - Retain values across procedure boundaries.
   - `g15`: Frame Pointer (FP) / Stack reference in standard ABI.
2. **Local Registers (`r0` - `r15`)**:
   - Dynamically allocated per stack frame on procedure calls (`call`, `callx`, `calls`).
   - `r0`: Previous Frame Pointer (PFP) & Return Status (`[31:6]` address, `[2:0]` return status).
   - `r1`: Stack Pointer (SP).
   - `r2`: Return Instruction Pointer (RIP).
   - `r3` – `r15`: General-purpose scratch registers for the current procedure.
3. **Register Window / Frame Cache**:
   - On-chip circular stack cache holding **4 to 8 procedure frames** (64 to 128 x 32-bit words).
   - **Hardware Spill/Fill Engine**: When calling beyond the frame cache depth, the oldest frame is automatically written out to memory at `PFP`. On `ret`, when the cache underflows, the previous frame is automatically reloaded from memory.
4. **Control & State Registers**:
   - **IP (Instruction Pointer)**: 32-bit program counter.
   - **AC (Arithmetic Controls)**: Condition code flags (CC2, CC1, CC0), integer overflow mask and sticky bits, rounding modes.
   - **PC (Process Controls)**: Execution mode (user/supervisor), state (stopped, executing, interrupted), trace control, interrupt priority.
   - **TC (Trace Controls)**: Hardware single-step, branch, call, return, and breakpoint tracing.

---

### 2.2 Instruction Encoding & Formats
All instructions are aligned to 32-bit word boundaries. The architecture defines 4 primary instruction formats:

```
1. REG Format (Register-to-Register Operations - 32-bit):
   +--------+---------+-------+-------+---+----+-------+-------+
   | Opcode |   dst   | src2  | src1  | 0 | M3 | M2 M1 | Op-ext|
   | (8b)   |  (5b)   | (5b)  | (5b)  |1b | 1b | 1b 1b |  (7b) |
   +--------+---------+-------+-------+---+----+-------+-------+
   * M1/M2 indicate whether src1/src2 are registers or 5-bit integer literals (0..31).
   * Operations: addo, addi, subo, subi, shlo, shro, shli, shri, rotate,
     all 16 boolean logic operations (and, or, xor, not, nand, nor, andnot, etc.),
     bit field operations (setbit, clrbit, notbit, alterbit, chkbit, extract, modify),
     multiplies/divides (mulo, muli, divo, divi, remo, remi).

2. COBR Format (Compare-and-Branch - 32-bit):
   +--------+---------+-------+-------+---+-------------------+
   | Opcode |  src2   | src1  |  M1   | S |  Displacement     |
   | (8b)   |  (5b)   | (5b)  | (1b)  |1b |     (13 bits)     |
   +--------+---------+-------+-------+---+-------------------+
   * Single-cycle compare and conditional jump with signed 13-bit word offset (±4096 words).
   * Operations: cmpbe, cmpbne, cmpbl, cmpble, cmpbg, cmpbge, bbc (branch bit clear), bbs.

3. CTRL Format (Control / Branch / Procedure Calls - 32-bit):
   +--------+-------------------------------------------------+
   | Opcode |               Target Displacement               |
   | (8b)   |                    (24 bits)                    |
   +--------+-------------------------------------------------+
   * Signed 24-bit word offset (±8M words = ±32MB addressing range).
   * Operations: b (branch), bal (branch-and-link), call, ret, calls (system call), fault.

4. MEM Format (Memory Load / Store / Effective Address - 32-bit or 64-bit):
   - Format MEMA (32-bit): Base register + 12-bit offset or IP-relative.
   - Format MEMB (64-bit): Base register + Index register with Scale factor (1, 2, 4, 8, 16) + 32-bit Displacement.
   * Operations:
     - Byte / Short: ldob, ldos, ldib, ldis, stob, stos, stib, stis.
     - Single Word (32-bit): ld, st, lda (load effective address).
     - Multi-Word: ldl/stl (2 words / 64-bit), ldt/stt (3 words / 96-bit), ldq/stq (4 words / 128-bit).
```

---

### 2.3 Microarchitecture Pipeline

A high-efficiency 5-stage synthesizable pipeline:

1. **IF (Instruction Fetch)**:
   - Instruction address generation (PC incrementer, branch predictor, branch target selector).
   - 32-bit / 64-bit prefetch queue.
2. **ID (Instruction Decode & Register Fetch)**:
   - REG / COBR / CTRL / MEM opcode decoding.
   - Global Regfile read (16 x 32-bit) & Local Regfile read (16 x 32-bit).
   - Immediate extraction & operand forwarding control.
   - Fast COBR comparator branch resolution.
3. **EX (Execute)**:
   - 32-bit ALU with full condition code generation (`AC` flags).
   - Multi-function bit manipulator (bit test, set, clear, extract, insert).
   - Multiplier/Divider unit (single-cycle or multi-cycle radix-4 integer engine).
   - Branch target resolution and branch mispredict recovery.
4. **MEM (Memory Access & Frame Management)**:
   - Address calculation for MEMA and MEMB modes (base + index * scale + disp).
   - Multi-word burst sequencer (handling `ldl`, `ldt`, `ldq`, `stl`, `stt`, `stq`).
   - Hardware Spill/Fill state machine for the local register window frame cache.
5. **WB (Writeback)**:
   - Writes results to Destination Global/Local register.
   - Updates `AC` and `PC` status registers.

---

### 2.4 Bus Interface Unit (BIU) & Drop-in Pinout

The BIU supports both modern Wishbone / AXI4-Lite interfaces for FPGA/SoC integration and the cycle-accurate Intel i960 L-Bus pin interface for drop-in board replacement:

- **Multiplexed Address/Data**: `LAD[31:0]` (or discrete `A[31:2]`, `D[31:0]`).
- **Byte Enables**: `BE3#`, `BE2#`, `BE1#`, `BE0#`.
- **Bus Control**: `ADS#` (Address Strobe), `READY#` (Data Ready input), `BLAST#` (Burst Last cycle indicator), `W/R#` (Write/Read indicator), `DEN#` (Data Enable), `DT/R#` (Data Transmit/Receive).
- **Interrupts & System**: `INT0#` - `INT3#`, `NMI#`, `RESET#`, `CLK2` / `CLKIN`.

---

## 3. Directory & File Structure to Implement

```
risc80960/
├── doc/
│   ├── isa_specification.md
│   ├── register_frames.md
│   └── bus_interface.md
├── rtl/
│   ├── core/
│   │   ├── open960_core.sv          # Top-level processor core
│   │   ├── open960_pkg.sv           # Opcodes, enums, constants, types
│   │   ├── open960_if_stage.sv      # Instruction fetch & PC logic
│   │   ├── open960_decoder.sv       # REG/COBR/CTRL/MEM decoder
│   │   ├── open960_regfile.sv       # Global (g0-g15) & Local (r0-r15) regfile
│   │   ├── open960_frame_cache.sv   # Register window frame cache (spill/fill FSM)
│   │   ├── open960_alu.sv           # ALU, bit manipulation, condition codes
│   │   ├── open960_muldiv.sv        # Integer multiply/divide unit
│   │   ├── open960_lsu.sv           # Load/Store unit with multi-word support
│   │   ├── open960_control_regs.sv  # AC, PC, TC register controls
│   │   └── open960_hazard_unit.sv   # Forwarding & pipeline stall logic
│   ├── bus/
│   │   ├── open960_lbus_biu.sv      # Intel i960 cycle-accurate L-Bus interface
│   │   ├── open960_wishbone_biu.sv  # Wishbone master interface for modern SoCs
│   │   └── open960_axi_biu.sv       # AXI4-Lite master interface
│   ├── top/
│   │   ├── open960_top_pga132.sv    # Pinout wrapper for 132-pin PGA drop-in
│   │   ├── open960_top_pqfp164.sv   # Pinout wrapper for 164-pin PQFP drop-in
│   │   └── open960_soc_top.sv       # SoC wrapper with RAM/ROM/UART for simulation
├── sim/
│   ├── cpp/
│   │   ├── verilator_main.cpp       # High-speed Verilator C++ test runner
│   │   └── i960_iss.cpp             # Reference C++ Instruction Set Simulator
│   ├── cocotb/
│   │   ├── test_alu.py              # ALU & Boolean test suite
│   │   ├── test_reg_windows.py      # Procedure call/ret frame cache tests
│   │   ├── test_decoder.py          # Decoder verification
│   │   ├── test_memory.py           # Multi-word & MEMB addressing tests
│   │   └── test_full_core.py        # Core execution testbench
├── sw/
│   ├── asm/                         # Assembly test programs & bootloader
│   │   ├── boot.s
│   │   ├── test_arithmetic.s
│   │   ├── test_call_ret.s
│   │   └── test_bubble_sort.s
│   ├── toolchain/                   # GCC / Binutils target configurations & scripts
│   │   └── build_i960_toolchain.sh
├── synth/
│   ├── yosys/
│   │   ├── synth_sky130.tcl         # Yosys synthesis script for Sky130
│   │   └── synth_ecp5.tcl           # Yosys synthesis script for Lattice ECP5
│   ├── openroad/
│   │   ├── config.mk
│   │   └── constraints.sdc
└── Makefile                         # Unified build & test Makefile
```

---

## 4. Phased Execution Plan

### Phase 1: Architecture Definition & Core RTL Skeleton
- Define `open960_pkg.sv` containing all instruction opcodes (REG, COBR, CTRL, MEM), status fields, and control constants.
- Implement the 5-stage pipeline modules: `open960_if_stage.sv`, `open960_decoder.sv`, `open960_alu.sv`, and `open960_hazard_unit.sv`.
- Implement `open960_regfile.sv` with dual-ported global (`g0-g15`) and local (`r0-r15`) registers.

### Phase 2: Frame Cache & Procedure Calling Mechanism
- Implement `open960_frame_cache.sv` with circular on-chip frame storage (e.g. 4 frames = 64 words).
- Implement the automatic spill and fill state machine handling `call`, `callx`, and `ret` instructions to seamlessly push/pop frames to/from memory via `PFP` and `SP`.

### Phase 3: Memory Subsystem & Complex Addressing Engine
- Implement `open960_lsu.sv` supporting:
  - Byte/Short/Word reads and writes with alignment and sign/zero extension (`ldib`, `ldob`, `ldis`, `ldos`).
  - Multi-word burst transfers: `ldl`/`stl` (64-bit), `ldt`/`stt` (96-bit), `ldq`/`stq` (128-bit).
  - Full MEMB addressing modes: Base + Index * Scale (1, 2, 4, 8, 16) + 32-bit Displacement.

### Phase 4: Control Registers, Interrupts, and Faults
- Implement `open960_control_regs.sv` for `AC` (Arithmetic Controls), `PC` (Process Controls), and condition code evaluation.
- Implement vector interrupt handler and fault table dispatch.

### Phase 5: Bus Interface Units & Pinout Wrappers
- Implement `open960_lbus_biu.sv` implementing the exact Intel i960 L-Bus cycle timing (`ADS#`, `READY#`, `BLAST#`, `DEN#`, `WR#`, `BE[3:0]#`).
- Implement top-level pin wrappers for drop-in physical compatibility (132-pin PGA / 164-pin PQFP) as well as Wishbone/AXI SoC wrappers.

### Phase 6: Verification & Simulation Suite
- Build Verilator harness and Cocotb test suite covering every instruction category with randomized and directed tests.
- Write reference assembly test programs (`test_arithmetic.s`, `test_call_ret.s`, `test_bubble_sort.s`) and run them cycle-by-cycle in simulation.

### Phase 7: Synthesis & OpenROAD Flow
- Provide synthesis scripts (`synth_sky130.tcl`, `synth_ecp5.tcl`) verifying zero timing violations, clean area reports, and RTL synthesis readiness.

---

## 5. Next Step

To begin generating the SystemVerilog RTL, testbenches, simulation models, and build infrastructure for this clean-room i960 duplicate, please **toggle to Act mode**.