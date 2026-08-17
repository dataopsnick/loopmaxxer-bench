// =============================================================================
// Open960 (RISC80960) - Verilator C++ Simulation Test Harness
// =============================================================================

#include <iostream>
#include <fstream>
#include <string>
#include <vector>
#include <iomanip>
#include <verilated.h>
#include <verilated_vcd_c.h>
#include "Vopen960_soc_top.h"

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);

    if (argc < 2) {
        std::cerr << "Usage: " << argv[0] << " <program.hex> [vcd_dump]" << std::endl;
        return 1;
    }

    std::string hex_file = argv[1];
    bool trace_enable = (argc > 2);

    std::cout << "[Open960 Sim] Loading HEX image: " << hex_file << std::endl;

    // Read HEX file into memory buffer
    std::ifstream infile(hex_file);
    if (!infile.is_open()) {
        std::cerr << "Error: Failed to open HEX file: " << hex_file << std::endl;
        return 1;
    }

    std::vector<uint32_t> code_words;
    std::string line;
    while (std::getline(infile, line)) {
        if (!line.empty() && line[0] != '#' && line[0] != '/') {
            uint32_t val = (uint32_t)std::stoul(line, nullptr, 16);
            code_words.push_back(val);
        }
    }
    infile.close();

    std::cout << "[Open960 Sim] Loaded " << code_words.size() * 4 << " bytes into ROM/RAM." << std::endl;

    auto* top = new Vopen960_soc_top;

    VerilatedVcdC* tfp = nullptr;
    if (trace_enable) {
        Verilated::traceEverOn(true);
        tfp = new VerilatedVcdC;
        top->trace(tfp, 99);
        tfp->open("sim_trace.vcd");
    }

    // Reset sequence
    top->clk = 0;
    top->rst_n = 0;
    for (int i = 0; i < 10; ++i) {
        top->clk = !top->clk;
        top->eval();
    }
    top->rst_n = 1;

    // Preload RAM
    for (size_t i = 0; i < code_words.size(); ++i) {
        // Access SoC internal RAM via Verilator hierarchy
        top->open960_soc_top->ram[i] = code_words[i];
    }

    std::cout << "[Open960 Sim] Starting processor execution..." << std::endl;
    std::cout << "--- UART Console Output ---" << std::endl;

    uint64_t max_cycles = 100000;
    uint64_t cycle = 0;
    bool pass = false;

    while (cycle < max_cycles && !Verilated::gotFinish()) {
        top->clk = 0;
        top->eval();
        if (tfp) tfp->dump((vluint64_t)(cycle * 2));

        top->clk = 1;
        top->eval();
        if (tfp) tfp->dump((vluint64_t)(cycle * 2 + 1));

        // Catch UART TX
        if (top->uart_tx_valid) {
            char c = (char)top->uart_tx_char;
            std::cout << c << std::flush;
        }

        // Catch Simulation Halt
        if (top->sim_halted) {
            std::cout << "\n---------------------------" << std::endl;
            if (top->sim_exit_code == 0) {
                std::cout << "[Open960 Sim] Execution PASSED (Exit code: 0) at cycle " << cycle << std::endl;
                pass = true;
            } else {
                std::cerr << "[Open960 Sim] Execution FAILED (Exit code: 0x" 
                          << std::hex << top->sim_exit_code << std::dec << ") at cycle " << cycle << std::endl;
                pass = false;
            }
            break;
        }

        cycle++;
    }

    if (cycle >= max_cycles) {
        std::cerr << "\n[Open960 Sim] ERROR: Simulation timed out after " << max_cycles << " cycles!" << std::endl;
        pass = false;
    }

    if (tfp) {
        tfp->close();
        delete tfp;
    }
    delete top;

    return pass ? 0 : 1;
}
