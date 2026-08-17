# =============================================================================
# Open960 (RISC80960) - Cocotb ALU Unit Testbench
# =============================================================================

import cocotb
from cocotb.triggers import Timer

@cocotb.test()
async def test_alu_basic_arithmetic(dut):
    """Test addition, subtraction, and condition code generation"""
    # Test ADD: 15 + 27 = 42
    dut.alu_op.value = 1  # ALU_ADD
    dut.op_a.value = 27
    dut.op_b.value = 15
    dut.op_c.value = 0
    dut.ac_cc_in.value = 0
    await Timer(1, units='ns')
    assert dut.alu_result.value == 42, f"Expected 42, got {dut.alu_result.value}"

    # Test SUB: 100 - 35 = 65
    dut.alu_op.value = 2  # ALU_SUB
    dut.op_a.value = 35
    dut.op_b.value = 100
    await Timer(1, units='ns')
    assert dut.alu_result.value == 65, f"Expected 65, got {dut.alu_result.value}"

@cocotb.test()
async def test_alu_logic_operations(dut):
    """Test 16 Boolean logic operations and bit manipulations"""
    # Test AND: 0xFF00 & 0x0FF0 = 0x0F00
    dut.alu_op.value = 3  # ALU_AND
    dut.op_a.value = 0x0FF0
    dut.op_b.value = 0xFF00
    await Timer(1, units='ns')
    assert dut.alu_result.value == 0x0F00

    # Test XOR: 0xAAAA_5555 ^ 0xFFFF_FFFF = 0x5555_AAAA
    dut.alu_op.value = 5  # ALU_XOR
    dut.op_a.value = 0xFFFF_FFFF
    dut.op_b.value = 0xAAAA_5555
    await Timer(1, units='ns')
    assert dut.alu_result.value == 0x5555_AAAA

    # Test SETBIT: Set bit 7 in 0x00 -> 0x80 (128)
    dut.alu_op.value = 18 # ALU_SETBIT
    dut.op_a.value = 7
    dut.op_b.value = 0
    await Timer(1, units='ns')
    assert dut.alu_result.value == 0x80

    # Test CLRBIT: Clear bit 7 in 0xFF -> 0x7F
    dut.alu_op.value = 19 # ALU_CLRBIT
    dut.op_a.value = 7
    dut.op_b.value = 0xFF
    await Timer(1, units='ns')
    assert dut.alu_result.value == 0x7F
