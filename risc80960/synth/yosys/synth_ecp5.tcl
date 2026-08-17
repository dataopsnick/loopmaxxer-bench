# =============================================================================
# Open960 (RISC80960) - Yosys Synthesis Script for Lattice ECP5 FPGA
# =============================================================================

# Read SystemVerilog sources
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_pkg.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_control_regs.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_regfile.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_frame_cache.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_alu.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_muldiv.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_decoder.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_lsu.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_if_stage.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_hazard_unit.sv
read_verilog -sv -I../../rtl/core ../../rtl/core/open960_core.sv

# Elaborate top module
hierarchy -check -top open960_core

# ECP5 Synthesis mapping
synth_ecp5 -top open960_core -json open960_core_ecp5.json
stat
