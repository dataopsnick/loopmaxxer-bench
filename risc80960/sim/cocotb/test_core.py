# =============================================================================
# Open960 (RISC80960) - Cocotb Core Full-Execution Testbench
# =============================================================================

import cocotb
from cocotb.clock import Clock
from cocotb.triggers import RisingEdge, Timer

@cocotb.test()
async def test_core_soc_boot(dut):
    """Test full Open960 SoC booting from ROM/RAM and executing program"""
    clock = Clock(dut.clk, 10, units="ns")
    cocotb.start_soon(clock.start())

    # Apply Reset
    dut.rst_n.value = 0
    for _ in range(5):
        await RisingEdge(dut.clk)
    dut.rst_n.value = 1

    uart_chars = []

    # Run for up to 2000 cycles or until halted
    for cycle in range(2000):
        await RisingEdge(dut.clk)
        if dut.uart_tx_valid.value == 1:
            char = chr(int(dut.uart_tx_char.value))
            uart_chars.append(char)
        if dut.sim_halted.value == 1:
            exit_code = int(dut.sim_exit_code.value)
            dut._log.info(f"Simulation halted at cycle {cycle} with exit code {exit_code}")
            dut._log.info(f"UART output: {''.join(uart_chars)}")
            assert exit_code == 0, f"Expected exit code 0, got {exit_code}"
            return

    dut._log.info(f"Completed 2000 test cycles. UART stream: {''.join(uart_chars)}")
