# =============================================================================
# Open960 (RISC80960) - SDC Timing Constraints
# Target: 50 MHz (20.0 ns period) / 100 MHz
# =============================================================================

current_design open960_core

# Primary Clock definition
create_clock -name clk -period 20.0 [get_ports clk]
set_clock_uncertainty 0.25 [get_clocks clk]
set_clock_transition 0.15 [get_clocks clk]

# Input / Output Delays
set_input_delay -max 3.0 -clock clk [all_inputs]
set_input_delay -min 0.5 -clock clk [all_inputs]

set_output_delay -max 3.0 -clock clk [all_outputs]
set_output_delay -min 0.5 -clock clk [all_outputs]

# Load and Driving Cell
set_load 0.05 [all_outputs]
