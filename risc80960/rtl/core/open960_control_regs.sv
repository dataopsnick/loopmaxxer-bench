// =============================================================================
// Open960 (RISC80960) - Control Registers Module (AC & PC)
// =============================================================================

`ifndef OPEN960_CONTROL_REGS_SV
`define OPEN960_CONTROL_REGS_SV

`include "open960_pkg.sv"

module open960_control_regs
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Condition Code / AC Updates from ALU / Execution
  input  logic        ac_cc_update_en,
  input  logic [2:0]  ac_cc_in,         // {cc_g, cc_e, cc_l}
  input  logic        ac_flags_update_en,
  input  logic        int_overflow_set,

  // Direct modac / modpc instruction updates
  input  logic        modac_en,
  input  logic [31:0] modac_mask,
  input  logic [31:0] modac_data,
  output logic [31:0] ac_reg_out,

  input  logic        modpc_en,
  input  logic [31:0] modpc_mask,
  input  logic [31:0] modpc_data,
  output logic [31:0] pc_reg_out,

  // Condition evaluation interface for branches
  input  logic [2:0]  branch_cond_type, // 000=no/f, 001=g, 010=e, 011=ge, 100=l, 101=ne, 110=le, 111=o/t
  output logic        branch_taken
);

  ac_reg_t ac_q, ac_d;
  pc_reg_t pc_q, pc_d;

  assign ac_reg_out = ac_q;
  assign pc_reg_out = pc_q;

  // Condition Code Evaluation
  // In i960:
  // 000 (no/false): 0
  // 001 (g):  cc_g (bit 2)
  // 010 (e):  cc_e (bit 1)
  // 011 (ge): cc_g | cc_e
  // 100 (l):  cc_l (bit 0)
  // 101 (ne): ~cc_e (or cc_g | cc_l)
  // 110 (le): cc_l | cc_e
  // 111 (o/true): 1
  always_comb begin
    case (branch_cond_type)
      3'b000: branch_taken = 1'b0;
      3'b001: branch_taken = ac_q.cc_g;
      3'b010: branch_taken = ac_q.cc_e;
      3'b011: branch_taken = ac_q.cc_g | ac_q.cc_e;
      3'b100: branch_taken = ac_q.cc_l;
      3'b101: branch_taken = ~ac_q.cc_e;
      3'b110: branch_taken = ac_q.cc_l | ac_q.cc_e;
      3'b111: branch_taken = 1'b1;
      default: branch_taken = 1'b0;
    endcase
  end

  // AC next-state logic
  always_comb begin
    ac_d = ac_q;

    if (ac_cc_update_en) begin
      ac_d.cc_g = ac_cc_in[2];
      ac_d.cc_e = ac_cc_in[1];
      ac_d.cc_l = ac_cc_in[0];
    end

    if (ac_flags_update_en && int_overflow_set) begin
      ac_d.int_overflow_flg = 1'b1;
    end

    if (modac_en) begin
      ac_d = (ac_q & ~modac_mask) | (modac_data & modac_mask);
    end
  end

  // PC next-state logic
  always_comb begin
    pc_d = pc_q;
    if (modpc_en) begin
      pc_d = (pc_q & ~modpc_mask) | (modpc_data & modpc_mask);
    end
  end

  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      ac_q <= '0;
      pc_q <= '0;
      pc_q.exec_mode <= 1'b1; // Boot in supervisor mode
    end else begin
      ac_q <= ac_d;
      pc_q <= pc_d;
    end
  end

endmodule : open960_control_regs

`endif // OPEN960_CONTROL_REGS_SV
