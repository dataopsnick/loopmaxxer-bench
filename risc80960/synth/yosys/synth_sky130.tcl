# =============================================================================
# Open960 (RISC80960) - Yosys Synthesis Script for SkyWater 130nm ASIC
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

# Optimization passes
proc
opt
fsm
opt
memory
opt

# Technology mapping
techmap
opt

# Print resource statistics
stat

# Output synthesized Verilog gate-level netlist
write_verilog -noattr open960_core_synth.v
