// =============================================================================
// Open960 (RISC80960) - Clean-Room C++ Reference Instruction Set Simulator (ISS)
// =============================================================================

#ifndef I960_ISS_HPP
#define I960_ISS_HPP

#include <cstdint>
#include <vector>
#include <iostream>
#include <array>
#include <cstring>

class Open960ISS {
public:
    uint32_t g[16];     // Global registers g0 - g15
    uint32_t r[16];     // Local registers r0 - r15
    uint32_t pc;        // Instruction pointer
    uint32_t ac;        // Arithmetic controls (CC bits: [2]=G, [1]=E, [0]=L)
    std::vector<uint8_t> mem;
    std::vector<std::array<uint32_t, 16>> frame_cache;

    Open960ISS(size_t mem_size = 64 * 1024) : pc(0), ac(0) {
        std::memset(g, 0, sizeof(g));
        std::memset(r, 0, sizeof(r));
        mem.resize(mem_size, 0);
    }

    void set_cc(bool g_flag, bool e_flag, bool l_flag) {
        ac = (ac & ~7) | ((g_flag ? 4 : 0) | (e_flag ? 2 : 0) | (l_flag ? 1 : 0));
    }

    uint32_t read_reg(uint32_t reg_idx) {
        if (reg_idx >= 16) return g[reg_idx - 16];
        return r[reg_idx];
    }

    void write_reg(uint32_t reg_idx, uint32_t val) {
        if (reg_idx >= 16) g[reg_idx - 16] = val;
        else r[reg_idx] = val;
    }

    uint32_t read32(uint32_t addr) {
        if (addr + 3 < mem.size()) {
            return mem[addr] | (mem[addr+1] << 8) | (mem[addr+2] << 16) | (mem[addr+3] << 24);
        }
        return 0;
    }

    void write32(uint32_t addr, uint32_t val) {
        if (addr + 3 < mem.size()) {
            mem[addr]   = val & 0xFF;
            mem[addr+1] = (val >> 8) & 0xFF;
            mem[addr+2] = (val >> 16) & 0xFF;
            mem[addr+3] = (val >> 24) & 0xFF;
        }
    }

    bool step() {
        uint32_t instr = read32(pc);
        uint8_t op = instr >> 24;

        if (op <= 0x1F) { // CTRL Format
            int32_t disp = (int32_t)(instr << 8) >> 6; // Sign-extend 24b and shift by 2
            uint32_t target = pc + disp;

            if (op == 0x10) { pc = target; return true; } // b
            if (op == 0x09) { // call
                std::array<uint32_t, 16> cur_frame;
                for (int i=0; i<16; ++i) cur_frame[i] = r[i];
                frame_cache.push_back(cur_frame);
                r[0] = g[15]; // PFP = FP
                r[2] = pc + 4; // RIP
                g[15] = (r[1] == 0) ? g[15] + 64 : (r[1] + 63) & ~63; // new FP
                r[1] = g[15] + 64; // new SP
                pc = target;
                return true;
            }
            if (op == 0x0A) { // ret
                if (!frame_cache.empty()) {
                    auto prev = frame_cache.back();
                    frame_cache.pop_back();
                    for (int i=0; i<16; ++i) r[i] = prev[i];
                }
                pc = r[2];
                return true;
            }
        }
        pc += 4;
        return true;
    }
};

#endif // I960_ISS_HPP
