#!/usr/bin/env python3
"""
Open960 Assembler (open960_as.py)
Clean-room two-pass assembler for Intel 80960 RISC microprocessor.
Generates 32-bit hex files for simulation ($readmemh) and binary ROM images.
"""

import sys
import re

REG_MAP = {
    # Local Registers
    'r0': 0, 'pfp': 0,
    'r1': 1, 'sp': 1,
    'r2': 2, 'rip': 2,
    'r3': 3, 'r4': 4, 'r5': 5, 'r6': 6, 'r7': 7,
    'r8': 8, 'r9': 9, 'r10': 10, 'r11': 11, 'r12': 12,
    'r13': 13, 'r14': 14, 'r15': 15,
    # Global Registers
    'g0': 16, 'g1': 17, 'g2': 18, 'g3': 19, 'g4': 20,
    'g5': 21, 'g6': 22, 'g7': 23, 'g8': 24, 'g9': 25,
    'g10': 26, 'g11': 27, 'g12': 28, 'g13': 29, 'g14': 30,
    'g15': 31, 'fp': 31
}

CTRL_OPS = {
    'call': 0x09, 'ret': 0x0A, 'bal': 0x0B, 'b': 0x10,
    'bno': 0x11, 'bg': 0x12, 'be': 0x13, 'bge': 0x14,
    'bl': 0x15, 'bne': 0x16, 'ble': 0x17, 'bo': 0x18, 'fault': 0x1A
}

COBR_OPS = {
    'cmpbno': 0x21, 'cmpbg': 0x22, 'cmpbe': 0x23, 'cmpbge': 0x24,
    'cmpbl': 0x25, 'cmpbne': 0x26, 'cmpble': 0x27, 'cmpbo': 0x28,
    'bbc': 0x30, 'bbs': 0x37,
    'cmpibe': 0x3A, 'cmpibne': 0x3B, 'cmpibl': 0x3C, 'cmpible': 0x3D,
    'cmpibg': 0x3E, 'cmpibge': 0x3F
}

MEM_OPS = {
    'ldob': 0x80, 'stob': 0x82, 'ldos': 0x88, 'stos': 0x8A,
    'lda': 0x8C, 'ld': 0x90, 'st': 0x92, 'ldl': 0x98, 'stl': 0x9A,
    'ldt': 0xA0, 'stt': 0xA2, 'ldq': 0xB0, 'stq': 0xB2,
    'ldib': 0xC0, 'stib': 0xC2, 'ldis': 0xC8, 'stis': 0xCA
}

REG_OPS = {
    'notbit': (0x58, 0x00), 'and': (0x58, 0x01), 'andnot': (0x58, 0x02),
    'setbit': (0x58, 0x03), 'notand': (0x58, 0x04), 'xor': (0x58, 0x06),
    'or': (0x58, 0x07), 'nor': (0x58, 0x08), 'xnor': (0x58, 0x09),
    'not': (0x58, 0x0A), 'ornot': (0x58, 0x0B), 'clrbit': (0x58, 0x0C),
    'notor': (0x58, 0x0D), 'nand': (0x58, 0x0E), 'alterbit': (0x58, 0x0F),
    'addo': (0x59, 0x20), 'addi': (0x59, 0x21), 'subo': (0x59, 0x22),
    'subi': (0x59, 0x23), 'cmpo': (0x5A, 0x28), 'cmpi': (0x5A, 0x29),
    'chkbit': (0x5A, 0x2C), 'shlo': (0x5C, 0x30), 'shro': (0x5C, 0x31),
    'shli': (0x5C, 0x32), 'shri': (0x5C, 0x33), 'rotate': (0x5C, 0x34),
    'mulo': (0x70, 0x40), 'muli': (0x70, 0x41), 'divo': (0x70, 0x42),
    'divi': (0x70, 0x43), 'remo': (0x70, 0x44), 'remi': (0x70, 0x45),
    'emul': (0x70, 0x46), 'ediv': (0x70, 0x47), 'mov': (0x5E, 0x50),
    'movl': (0x5E, 0x51), 'movt': (0x5E, 0x52), 'movq': (0x5E, 0x53),
    'modpc': (0x60, 0x61), 'modac': (0x60, 0x62), 'flushreg': (0x60, 0x63)
}
def parse_val(token, labels=None, curr_pc=0):
    token = token.strip()
    if labels and token in labels:
        return labels[token]
    if token.startswith('0x') or token.startswith('0X'):
        return int(token, 16)
    if token.isdigit() or (token.startswith('-') and token[1:].isdigit()):
        return int(token, 10)
    return 0

def parse_reg(token):
    token = token.strip().lower()
    return REG_MAP.get(token, 0)

def assemble_line(line, pc, labels):
    line = line.split('#')[0].split(';')[0].strip()
    if not line:
        return []
    
    parts = re.split(r'[\s,]+', line)
    mnemonic = parts[0].lower()
    args = parts[1:]

    # CTRL Format
    if mnemonic in CTRL_OPS:
        op = CTRL_OPS[mnemonic]
        target = parse_val(args[0], labels, pc) if args else 0
        disp = (target - pc) >> 2
        word = (op << 24) | (disp & 0x00FFFFFF)
        return [word]

    # COBR Format: mnemonic src1, src2, target
    if mnemonic in COBR_OPS:
        op = COBR_OPS[mnemonic]
        src1_token, src2_token, target_token = args[0], args[1], args[2]
        target = parse_val(target_token, labels, pc)
        disp = ((target - pc) >> 2) & 0x1FFF
        m1 = 1 if (src1_token not in REG_MAP and not src1_token.startswith('r') and not src1_token.startswith('g')) else 0
        src1 = parse_val(src1_token) if m1 else parse_reg(src1_token)
        src2 = parse_reg(src2_token)
        word = (op << 24) | ((src2 & 0x1F) << 19) | ((src1 & 0x1F) << 14) | (m1 << 13) | disp
        return [word]

    # MEM Format: ld/st disp(base), dst or ld (base), dst
    if mnemonic in MEM_OPS:
        op = MEM_OPS[mnemonic]
        mem_operand = args[0]
        dst = parse_reg(args[1]) if len(args) > 1 else 0
        
        # Check for disp(base) or (base)
        m = re.match(r'([-\w\d]*)\(([\w\d]+)\)', mem_operand)
        if m:
            disp_str, base_str = m.group(1), m.group(2)
            disp = parse_val(disp_str, labels, pc) if disp_str else 0
            base = parse_reg(base_str)
            word = (op << 24) | ((dst & 0x1F) << 19) | ((base & 0x1F) << 14) | (disp & 0xFFF)
            return [word]
        else:
            disp = parse_val(mem_operand, labels, pc)
            word = (op << 24) | ((dst & 0x1F) << 19) | (disp & 0xFFF)
            return [word]

    # REG Format: mnemonic src1, src2, dst
    if mnemonic in REG_OPS:
        op_fam, op_ext = REG_OPS[mnemonic]
        if len(args) == 3:
            src1_tok, src2_tok, dst_tok = args[0], args[1], args[2]
            m1 = 1 if (src1_tok not in REG_MAP and not src1_tok.startswith('r') and not src1_tok.startswith('g')) else 0
            m2 = 1 if (src2_tok not in REG_MAP and not src2_tok.startswith('r') and not src2_tok.startswith('g')) else 0
            src1 = parse_val(src1_tok) if m1 else parse_reg(src1_tok)
            src2 = parse_val(src2_tok) if m2 else parse_reg(src2_tok)
            dst = parse_reg(dst_tok)
        elif len(args) == 2:
            src1_tok, dst_tok = args[0], args[1]
            m1 = 1 if (src1_tok not in REG_MAP and not src1_tok.startswith('r') and not src1_tok.startswith('g')) else 0
            m2 = 0
            src1 = parse_val(src1_tok) if m1 else parse_reg(src1_tok)
            src2 = 0
            dst = parse_reg(dst_tok)
        else:
            src1, src2, dst, m1, m2 = 0, 0, 0, 0, 0

        word = (op_fam << 24) | ((dst & 0x1F) << 19) | ((src2 & 0x1F) << 14) | ((src1 & 0x1F) << 9) | (m2 << 6) | (m1 << 5) | (op_ext & 0x1F)
        return [word]

    # Direct data words (.word / .long)
    if mnemonic in ['.word', '.long']:
        return [parse_val(arg, labels, pc) for arg in args]

    return []

def assemble_file(input_path, output_hex, output_bin=None):
    with open(input_path, 'r') as f:
        lines = f.readlines()

    # Pass 1: Collect Labels
    labels = {}
    pc = 0
    for line in lines:
        raw = line.split('#')[0].split(';')[0].strip()
        if not raw:
            continue
        if ':' in raw:
            label, rest = raw.split(':', 1)
            labels[label.strip()] = pc
            raw = rest.strip()
        if raw:
            words = assemble_line(raw, pc, {})
            pc += len(words) * 4

    # Pass 2: Generate Code
    code = []
    pc = 0
    for line in lines:
        raw = line.split('#')[0].split(';')[0].strip()
        if not raw:
            continue
        if ':' in raw:
            _, rest = raw.split(':', 1)
            raw = rest.strip()
        if raw:
            words = assemble_line(raw, pc, labels)
            code.extend(words)
            pc += len(words) * 4

    # Write HEX output ($readmemh format)
    with open(output_hex, 'w') as f:
        for w in code:
            f.write(f"{w:08x}\n")

    # Write binary if requested
    if output_bin:
        with open(output_bin, 'wb') as f:
            for w in code:
                f.write(w.to_bytes(4, byteorder='little'))

    print(f"[Open960-AS] Assembled {input_path} -> {output_hex} ({len(code)*4} bytes)")

if __name__ == '__main__':
    if len(sys.argv) < 3:
        print("Usage: open960_as.py <input.s> <output.hex> [output.bin]")
        sys.exit(1)
    bin_out = sys.argv[3] if len(sys.argv) > 3 else None
    assemble_file(sys.argv[1], sys.argv[2], bin_out)
