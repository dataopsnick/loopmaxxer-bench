// =============================================================================
// Open960 (RISC80960) - Instruction Fetch (IF) & PC Generation Unit
// =============================================================================

`ifndef OPEN960_IF_STAGE_SV
`define OPEN960_IF_STAGE_SV

`include "open960_pkg.sv"

module open960_if_stage
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Pipeline Flow Control
  input  logic        stall,
  input  logic        flush,
  input  logic        branch_taken,
  input  logic [31:0] branch_target,

  // Outputs to Decode (ID) Stage
  output logic [31:0] if_pc,
  output logic [31:0] if_instr_w0,
  output logic [31:0] if_instr_w1,
  output logic        if_valid,
  output logic        if_busy,

  // Instruction Bus Interface
  output bus_req_t    ibus_req,
  input  bus_resp_t   ibus_resp
);

  typedef enum logic [1:0] {
    FETCH_W0, FETCH_W1, FETCH_DONE
  } if_state_e;

  if_state_e state_q, state_d;
  logic [31:0] pc_q, pc_d;
  logic [31:0] w0_reg, w1_reg;
  logic        is_memb_word0;

  // Identify MEMB format from opcode and bit 13 of word 0
  assign is_memb_word0 = (w0_reg[31:28] >= 4'h8 && w0_reg[31:28] <= 4'hC) && w0_reg[13];
  assign if_pc         = pc_q;
  assign if_instr_w0   = w0_reg;
  assign if_instr_w1   = w1_reg;
  assign if_busy       = (state_q != FETCH_DONE);

  always_comb begin
    state_d          = state_q;
    pc_d             = pc_q;
    if_valid         = 1'b0;
    ibus_req.valid   = 1'b0;
    ibus_req.write_en= 1'b0;
    ibus_req.addr    = pc_q;
    ibus_req.wdata   = 32'h0;
    ibus_req.byte_en = 4'b1111;
    ibus_req.size    = BUS_SIZE_WORD;
    ibus_req.burst_len = 2'd0;

    if (flush || branch_taken) begin
      state_d        = FETCH_W0;
      pc_d           = branch_target;
    end else begin
      case (state_q)
        FETCH_W0: begin
          ibus_req.valid = 1'b1;
          ibus_req.addr  = pc_q;
          if (ibus_resp.ready) begin
            if (ibus_resp.rdata[31:28] >= 4'h8 && ibus_resp.rdata[31:28] <= 4'hC && ibus_resp.rdata[13]) begin
              // MEMB: need to fetch second 32-bit word
              state_d = FETCH_W1;
            end else begin
              state_d = FETCH_DONE;
            end
          end
        end

        FETCH_W1: begin
          ibus_req.valid = 1'b1;
          ibus_req.addr  = pc_q + 32'd4;
          if (ibus_resp.ready) begin
            state_d = FETCH_DONE;
          end
        end

        FETCH_DONE: begin
          if_valid = 1'b1;
          if (!stall) begin
            pc_d    = pc_q + (is_memb_word0 ? 32'd8 : 32'd4);
            state_d = FETCH_W0;
          end
        end

        default: state_d = FETCH_W0;
      endcase
    end
  end

  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      state_q <= FETCH_W0;
      pc_q    <= RESET_VECTOR;
      w0_reg  <= 32'h0;
      w1_reg  <= 32'h0;
    end else begin
      state_q <= state_d;
      pc_q    <= pc_d;

      if (state_q == FETCH_W0 && ibus_resp.ready) begin
        w0_reg <= ibus_resp.rdata;
        w1_reg <= 32'h0;
      end else if (state_q == FETCH_W1 && ibus_resp.ready) begin
        w1_reg <= ibus_resp.rdata;
      end
    end
  end

endmodule : open960_if_stage

`endif // OPEN960_IF_STAGE_SV
