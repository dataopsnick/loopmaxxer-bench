# Open960 (RISC80960)

**Open-source, synthesizable SystemVerilog clean-room duplicate of the Intel 80960 (i960) 32-bit RISC microprocessor.**

---

## Key Features
- **Clean-Room Architecture**: Developed strictly from public architectural specifications and reference documentation; 100% unencumbered by proprietary microcode or netlists.
- **Full ISA Coverage**: Implements all 4 primary instruction formats (REG, COBR, CTRL, MEM/MEMB) including 16 boolean logic operations, arithmetic, bitfield operations, integer multiply/divide, and conditional branches.
- **Hardware Register Windows**: Dual register file architecture with 16 Global registers (`g0`-`g15`) and 16 Local registers (`r0`-`r15`), backed by an on-chip Frame Cache with automatic hardware Spill/Fill FSM.
- **Memory & Addressing**: Complete MEMA and MEMB addressing modes (base + index * scale + disp32) and multi-word burst operations (`ldl`/`stl`, `ldt`/`stt`, `ldq`/`stq`).
- **Interfaces**:
  - Cycle-accurate Intel i960 L-Bus with 132-pin PGA drop-in package wrapper.
  - Standard Wishbone B4 and AXI4-Lite master interfaces for modern FPGA/ASIC SoC designs.
- **Tooling & Verification**:
  - Custom two-pass Python assembler (`sw/tools/open960_as.py`).
  - High-speed C++ Verilator testbench and ISS reference model.
  - Cocotb SystemVerilog verification suite.
  - ASIC synthesis scripts for SkyWater 130nm (`sky130`) and Lattice ECP5 FPGA via Yosys & OpenROAD.

---

## Directory Layout
```
risc80960/
├── doc/                        # Architectural documentation & bus timing specs
├── rtl/
│   ├── core/                  # Core pipeline, ALU, decoder, LSU, frame cache
│   ├── bus/                   # L-Bus, Wishbone, and AXI BIU adapters
│   └── top/                   # 132-PGA drop-in package & SoC simulation wrappers
├── sim/
│   ├── cpp/                   # Verilator C++ testbench & C++ ISS simulator
│   └── cocotb/                # Cocotb verification tests
├── sw/
│   ├── asm/                   # Reference test programs (boot, arithmetic, sort)
│   └── tools/                 # Open960 two-pass assembler
├── synth/
│   ├── yosys/                 # SkyWater 130nm & Lattice ECP5 synthesis scripts
│   └── openroad/              # SDC timing constraints
├── Makefile
└── README.md
```

---

## Quickstart

### 1. Assemble Test Programs
```bash
make asm
```

### 2. Run Verification with Verilator
```bash
make test
```

### 3. Synthesize for SkyWater 130nm ASIC
```bash
make synth_sky130
```
