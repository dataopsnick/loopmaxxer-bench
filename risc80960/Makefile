# =============================================================================
# Open960 (RISC80960) - Top-Level Project Makefile
# =============================================================================

PYTHON ?= python3
VERILATOR ?= verilator
YOSYS ?= yosys

SRC_DIR = rtl
CORE_SRCS = $(SRC_DIR)/core/open960_pkg.sv \
            $(SRC_DIR)/core/open960_control_regs.sv \
            $(SRC_DIR)/core/open960_regfile.sv \
            $(SRC_DIR)/core/open960_frame_cache.sv \
            $(SRC_DIR)/core/open960_alu.sv \
            $(SRC_DIR)/core/open960_muldiv.sv \
            $(SRC_DIR)/core/open960_decoder.sv \
            $(SRC_DIR)/core/open960_lsu.sv \
            $(SRC_DIR)/core/open960_if_stage.sv \
            $(SRC_DIR)/core/open960_hazard_unit.sv \
            $(SRC_DIR)/core/open960_core.sv \
            $(SRC_DIR)/top/open960_soc_top.sv

ASM_SRCS = sw/asm/boot.s \
           sw/asm/test_arithmetic.s \
           sw/asm/test_call_ret.s \
           sw/asm/test_bubble_sort.s

HEX_FILES = $(ASM_SRCS:.s=.hex)

.PHONY: all asm build_sim run_boot test synth_sky130 synth_ecp5 clean

all: asm build_sim run_boot

# Assemble all assembly test programs
asm: $(HEX_FILES)

%.hex: %.s sw/tools/open960_as.py
	@mkdir -p $(dir $@)
	$(PYTHON) sw/tools/open960_as.py $< $@

# Build Verilator simulation executable
build_sim: asm
	@mkdir -p build/verilator
	$(VERILATOR) --cc --exe --trace \
		-Irtl/core -Irtl/bus -Irtl/top \
		--top-module open960_soc_top \
		rtl/top/open960_soc_top.sv \
		sim/cpp/verilator_main.cpp \
		--Mdir build/verilator \
		-o sim_open960
	$(MAKE) -C build/verilator -f Vopen960_soc_top.mk

# Run boot program in simulation
run_boot: build_sim sw/asm/boot.hex
	./build/verilator/sim_open960 sw/asm/boot.hex

# Run full test suite
test: build_sim $(HEX_FILES)
	@echo "=== Running Boot Test ==="
	./build/verilator/sim_open960 sw/asm/boot.hex
	@echo "=== Running Arithmetic Test ==="
	./build/verilator/sim_open960 sw/asm/test_arithmetic.hex
	@echo "=== Running Call/Return Frame Cache Test ==="
	./build/verilator/sim_open960 sw/asm/test_call_ret.hex
	@echo "=== Running Bubble Sort Test ==="
	./build/verilator/sim_open960 sw/asm/test_bubble_sort.hex
	@echo "=== ALL OPEN960 TESTS PASSED ==="

# Yosys synthesis
synth_sky130:
	cd synth/yosys && $(YOSYS) -s synth_sky130.tcl

synth_ecp5:
	cd synth/yosys && $(YOSYS) -s synth_ecp5.tcl

clean:
	rm -rf build/ $(HEX_FILES) synth/yosys/*.v synth/yosys/*.json *.vcd
