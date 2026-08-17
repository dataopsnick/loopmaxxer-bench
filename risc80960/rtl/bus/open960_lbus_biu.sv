// =============================================================================
// Open960 (RISC80960) - Cycle-Accurate Intel 80960 L-Bus BIU
// =============================================================================

`ifndef OPEN960_LBUS_BIU_SV
`define OPEN960_LBUS_BIU_SV

`include "open960_pkg.sv"

module open960_lbus_biu
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Core internal interface
  input  bus_req_t    core_req,
  output bus_resp_t   core_resp,

  // Physical Intel i960 L-Bus Signals
  inout  wire  [31:0] lad,          // Multiplexed Local Address/Data Bus
  output logic [3:0]  be_n,         // Byte Enables (active low)
  output logic        ads_n,        // Address Strobe (active low)
  input  logic        ready_n,      // Transfer Ready (active low)
  output logic        blast_n,      // Burst Last indicator (active low)
  output logic        w_r_n,        // Write / Read indicator (1=Write, 0=Read)
  output logic        den_n,        // Data Enable (active low)
  output logic        dt_r_n        // Data Transmit / Receive
);

  typedef enum logic [2:0] {
    ST_IDLE, ST_T1_ADDR, ST_T2_DATA, ST_BURST_NEXT
  } lbus_state_e;

  lbus_state_e state_q, state_d;
  logic [1:0]  burst_cnt_q, burst_cnt_d;
  logic [31:0] lad_out;
  logic        lad_oe;
  logic [31:0] rdata_captured;

  assign lad = lad_oe ? lad_out : 32'hZZZZ_ZZZZ;

  always_comb begin
    state_d          = state_q;
    burst_cnt_d      = burst_cnt_q;
    core_resp.ready  = 1'b0;
    core_resp.rdata  = rdata_captured;
    core_resp.error  = 1'b0;
    ads_n            = 1'b1;
    blast_n          = 1'b1;
    w_r_n            = 1'b0;
    den_n            = 1'b1;
    dt_r_n           = 1'b0;
    lad_oe           = 1'b0;
    lad_out          = 32'h0;
    be_n             = ~core_req.byte_en;

    case (state_q)
      ST_IDLE: begin
        if (core_req.valid) begin
          state_d     = ST_T1_ADDR;
          burst_cnt_d = core_req.burst_len;
        end
      end

      ST_T1_ADDR: begin
        ads_n   = 1'b0;
        w_r_n   = core_req.write_en;
        dt_r_n  = core_req.write_en;
        lad_oe  = 1'b1;
        lad_out = core_req.addr;
        state_d = ST_T2_DATA;
      end

      ST_T2_DATA: begin
        den_n  = 1'b0;
        dt_r_n = core_req.write_en;
        w_r_n  = core_req.write_en;

        if (burst_cnt_q == 2'd0) begin
          blast_n = 1'b0;
        end

        if (core_req.write_en) begin
          lad_oe  = 1'b1;
          lad_out = core_req.wdata;
        end

        if (!ready_n) begin
          if (burst_cnt_q == 2'd0) begin
            core_resp.ready = 1'b1;
            core_resp.rdata = lad;
            state_d         = ST_IDLE;
          end else begin
            burst_cnt_d = burst_cnt_q - 1'b1;
            state_d     = ST_BURST_NEXT;
          end
        end
      end

      ST_BURST_NEXT: begin
        state_d = ST_T2_DATA;
      end

      default: state_d = ST_IDLE;
    endcase
  end

  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      state_q        <= ST_IDLE;
      burst_cnt_q    <= 2'd0;
      rdata_captured <= 32'h0;
    end else begin
      state_q     <= state_d;
      burst_cnt_q <= burst_cnt_d;
      if (state_q == ST_T2_DATA && !ready_n && !core_req.write_en) begin
        rdata_captured <= lad;
      end
    end
  end

endmodule : open960_lbus_biu

`endif // OPEN960_LBUS_BIU_SV
