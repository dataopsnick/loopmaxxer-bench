# Open960 (RISC80960) Instruction Set Architecture Specification

## 1. Overview
The Open960 is an open-source, synthesizable SystemVerilog clean-room core implementing the 32-bit Intel 80960 (i960) RISC microprocessor architecture.

## 2. Register Organization
The core provides 32 general-purpose 32-bit registers divided into two sets:
- **Global Registers (`g0` - `g15`)**: Fixed across procedure boundaries.
  - `g14`: Branch and Link return pointer (`bal`).
  - `g15` (`fp`): Current Frame Pointer.
- **Local Registers (`r0` - `r15`)**: Dynamically allocated per stack frame on procedure calls.
  - `r0` (`pfp`): Previous Frame Pointer and return status flags.
  - `r1` (`sp`): Stack Pointer.
  - `r2` (`rip`): Return Instruction Pointer.
  - `r3` - `r15`: General-purpose procedure-local scratch registers.

## 3. Instruction Encoding Formats
All instructions are aligned on 32-bit word boundaries.
- **CTRL Format (32-bit)**: Opcode (8b) + Signed 24-bit Word Displacement.
  - Operations: `b`, `be`, `bne`, `bl`, `ble`, `bg`, `bge`, `bo`, `bno`, `call`, `ret`, `bal`, `fault`.
- **COBR Format (32-bit)**: Opcode (8b) + `src2` (5b) + `src1` (5b) + `M1` (1b) + Signed 13-bit Displacement.
  - Operations: `cmpbe`, `cmpbne`, `cmpbl`, `cmpble`, `cmpbg`, `cmpbge`, `cmpibe`, `cmpibne`, `cmpibl`, `cmpible`, `cmpibg`, `cmpibge`, `bbc`, `bbs`.
- **MEMA Format (32-bit)**: Opcode (8b) + `dst/src` (5b) + `base` (5b) + Mode (1b) + 12-bit Displacement.
- **MEMB Format (64-bit)**:
  - Word 0: Opcode (8b) + `dst/src` (5b) + `base` (5b) + Mode=1 (1b) + Scale (3b) + `index` (5b).
  - Word 1: 32-bit signed displacement.
- **REG Format (32-bit)**: Opcode (8b) + `dst` (5b) + `src2` (5b) + `src1` (5b) + Mode flags + Opcode Extension (7b).
  - 16 Boolean logic operations, arithmetic, bitfield manipulation, multiply, divide, shift/rotate.
