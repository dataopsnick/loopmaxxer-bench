// =============================================================================
// Open960 (RISC80960) - Pipeline Hazard Detection & Forwarding Unit
// =============================================================================

`ifndef OPEN960_HAZARD_UNIT_SV
`define OPEN960_HAZARD_UNIT_SV

`include "open960_pkg.sv"

module open960_hazard_unit
  import open960_pkg::*;
(
  // Operand Registers from ID stage
  input  logic [4:0]  id_src1_reg,
  input  logic [4:0]  id_src2_reg,
  input  logic [4:0]  id_src3_reg,
  input  logic        id_uses_src1,
  input  logic        id_uses_src2,
  input  logic        id_uses_src3,

  // EX Stage Register State
  input  logic [4:0]  ex_dst_reg,
  input  logic        ex_reg_write_en,
  input  logic        ex_is_load,
  input  logic        ex_is_multi_word,
  input  logic [1:0]  ex_multi_count,

  // MEM Stage Register State
  input  logic [4:0]  mem_dst_reg,
  input  logic        mem_reg_write_en,
  input  logic        mem_is_multi_word,
  input  logic [1:0]  mem_multi_count,

  // WB Stage Register State
  input  logic [4:0]  wb_dst_reg,
  input  logic        wb_reg_write_en,

  // Subsystem Busy Flags
  input  logic        muldiv_busy,
  input  logic        lsu_busy,
  input  logic        frame_cache_busy,
  input  logic        branch_taken,

  // Forwarding Multiplexer Selects
  // 00 = Regfile original, 01 = Forward from EX, 10 = Forward from MEM, 11 = Forward from WB
  output logic [1:0]  fwd_src1_sel,
  output logic [1:0]  fwd_src2_sel,
  output logic [1:0]  fwd_src3_sel,

  // Pipeline Flow Control Signals
  output logic        stall_if,
  output logic        stall_id,
  output logic        stall_ex,
  output logic        flush_id,
  output logic        flush_ex
);

  logic load_use_hazard;

  // Load-Use Detection: ID stage needs data from a load currently in EX
  always_comb begin
    load_use_hazard = 1'b0;
    if (ex_is_load && ex_reg_write_en) begin
      if (id_uses_src1 && (id_src1_reg == ex_dst_reg)) load_use_hazard = 1'b1;
      if (id_uses_src2 && (id_src2_reg == ex_dst_reg)) load_use_hazard = 1'b1;
      if (id_uses_src3 && (id_src3_reg == ex_dst_reg)) load_use_hazard = 1'b1;
    end
  end

  // Forwarding Logic for src1
  always_comb begin
    if (id_uses_src1 && ex_reg_write_en && (id_src1_reg == ex_dst_reg)) begin
      fwd_src1_sel = 2'b01; // EX forwarding
    end else if (id_uses_src1 && mem_reg_write_en && (id_src1_reg == mem_dst_reg)) begin
      fwd_src1_sel = 2'b10; // MEM forwarding
    end else if (id_uses_src1 && wb_reg_write_en && (id_src1_reg == wb_dst_reg)) begin
      fwd_src1_sel = 2'b11; // WB forwarding
    end else begin
      fwd_src1_sel = 2'b00;
    end
  end

  // Forwarding Logic for src2
  always_comb begin
    if (id_uses_src2 && ex_reg_write_en && (id_src2_reg == ex_dst_reg)) begin
      fwd_src2_sel = 2'b01;
    end else if (id_uses_src2 && mem_reg_write_en && (id_src2_reg == mem_dst_reg)) begin
      fwd_src2_sel = 2'b10;
    end else if (id_uses_src2 && wb_reg_write_en && (id_src2_reg == wb_dst_reg)) begin
      fwd_src2_sel = 2'b11;
    end else begin
      fwd_src2_sel = 2'b00;
    end
  end

  // Forwarding Logic for src3
  always_comb begin
    if (id_uses_src3 && ex_reg_write_en && (id_src3_reg == ex_dst_reg)) begin
      fwd_src3_sel = 2'b01;
    end else if (id_uses_src3 && mem_reg_write_en && (id_src3_reg == mem_dst_reg)) begin
      fwd_src3_sel = 2'b10;
    end else if (id_uses_src3 && wb_reg_write_en && (id_src3_reg == wb_dst_reg)) begin
      fwd_src3_sel = 2'b11;
    end else begin
      fwd_src3_sel = 2'b00;
    end
  end

  // Pipeline Stall and Flush Generation
  always_comb begin
    stall_if = 1'b0;
    stall_id = 1'b0;
    stall_ex = 1'b0;
    flush_id = 1'b0;
    flush_ex = 1'b0;

    if (frame_cache_busy || muldiv_busy || lsu_busy) begin
      stall_if = 1'b1;
      stall_id = 1'b1;
      stall_ex = 1'b1;
    end else if (load_use_hazard) begin
      stall_if = 1'b1;
      stall_id = 1'b1;
      flush_ex = 1'b1; // Insert bubble into EX
    end else if (branch_taken) begin
      flush_id = 1'b1; // Flush speculative instruction in ID
    end
  end

endmodule : open960_hazard_unit

`endif // OPEN960_HAZARD_UNIT_SV
