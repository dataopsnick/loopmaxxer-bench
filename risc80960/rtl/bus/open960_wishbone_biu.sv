// =============================================================================
// Open960 (RISC80960) - Wishbone B4 Master BIU
// =============================================================================

`ifndef OPEN960_WISHBONE_BIU_SV
`define OPEN960_WISHBONE_BIU_SV

`include "open960_pkg.sv"

module open960_wishbone_biu
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Core internal interface
  input  bus_req_t    core_req,
  output bus_resp_t   core_resp,

  // Wishbone Master Interface
  output logic [31:0] wb_adr_o,
  output logic [31:0] wb_dat_o,
  input  logic [31:0] wb_dat_i,
  output logic [3:0]  wb_sel_o,
  output logic        wb_we_o,
  output logic        wb_cyc_o,
  output logic        wb_stb_o,
  input  logic        wb_ack_i,
  input  logic        wb_err_i
);

  typedef enum logic [1:0] {
    WB_IDLE, WB_ACTIVE
  } wb_state_e;

  wb_state_e state_q, state_d;
  logic [31:0] rdata_captured;

  assign wb_adr_o = core_req.addr;
  assign wb_dat_o = core_req.wdata;
  assign wb_sel_o = core_req.byte_en;
  assign wb_we_o  = core_req.write_en;

  always_comb begin
    state_d         = state_q;
    wb_cyc_o        = 1'b0;
    wb_stb_o        = 1'b0;
    core_resp.ready = 1'b0;
    core_resp.rdata = wb_dat_i;
    core_resp.error = wb_err_i;

    case (state_q)
      WB_IDLE: begin
        if (core_req.valid) begin
          wb_cyc_o = 1'b1;
          wb_stb_o = 1'b1;
          state_d  = WB_ACTIVE;
        end
      end

      WB_ACTIVE: begin
        wb_cyc_o = 1'b1;
        wb_stb_o = 1'b1;
        if (wb_ack_i || wb_err_i) begin
          core_resp.ready = 1'b1;
          core_resp.rdata = wb_dat_i;
          core_resp.error = wb_err_i;
          state_d         = WB_IDLE;
        end
      end

      default: state_d = WB_IDLE;
    endcase
  end

  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      state_q <= WB_IDLE;
    end else begin
      state_q <= state_d;
    end
  end

endmodule : open960_wishbone_biu

`endif // OPEN960_WISHBONE_BIU_SV
