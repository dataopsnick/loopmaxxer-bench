// =============================================================================
// Open960 (RISC80960) - Load/Store Unit (LSU) with Multi-Word Burst Engine
// =============================================================================

`ifndef OPEN960_LSU_SV
`define OPEN960_LSU_SV

`include "open960_pkg.sv"

module open960_lsu
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Execution Stage Requests
  input  logic        req_valid,
  input  logic        is_load,
  input  logic        is_store,
  input  logic        is_multi_word,
  input  logic [1:0]  multi_word_count, // 0=1w, 1=2w, 2=3w, 3=4w
  input  logic [1:0]  mem_size,         // 0=byte, 1=short, 2=word, 3=quad
  input  logic        mem_sign_extend,
  input  logic [31:0] base_val,
  input  logic [31:0] index_val,
  input  logic [2:0]  scale_factor,     // 0=1, 1=2, 2=4, 3=8, 4=16
  input  logic [31:0] disp_val,
  input  logic [31:0] wdata_word0,
  input  logic [31:0] wdata_word1,
  input  logic [31:0] wdata_word2,
  input  logic [31:0] wdata_word3,

  // Status to Pipeline
  output logic        lsu_busy,
  output logic        lsu_done,
  output logic [31:0] rdata_word0,
  output logic [31:0] rdata_word1,
  output logic [31:0] rdata_word2,
  output logic [31:0] rdata_word3,
  output logic [31:0] effective_addr,

  // Bus Interface Unit Handshake
  output bus_req_t    bus_req,
  input  bus_resp_t   bus_resp
);

  logic [31:0] scaled_index;
  logic [31:0] calc_addr;
  logic [1:0]  burst_cnt_q, burst_cnt_d;
  logic [1:0]  target_words;
  logic [31:0] rdata_reg [0:3];

  assign target_words = is_multi_word ? multi_word_count : 2'd0;

  // Scale factor decoder
  typedef enum logic [1:0] {
    LSU_IDLE, LSU_BUS_ACTIVE, LSU_DONE_WAIT
  } lsu_state_e;

  lsu_state_e state_q, state_d;

  assign lsu_busy = (state_q != LSU_IDLE) || req_valid;
  assign rdata_word0 = rdata_reg[0];
  assign rdata_word1 = rdata_reg[1];
  assign rdata_word2 = rdata_reg[2];
  assign rdata_word3 = rdata_reg[3];

  // Byte enable generation
  logic [3:0] gen_byte_en;
  logic [31:0] gen_wdata;

  always_comb begin
    gen_byte_en = 4'b1111;
    gen_wdata   = wdata_word0;
    
    if (burst_cnt_q == 2'd1) gen_wdata = wdata_word1;
    else if (burst_cnt_q == 2'd2) gen_wdata = wdata_word2;
    else if (burst_cnt_q == 2'd3) gen_wdata = wdata_word3;

    if (!is_multi_word) begin
      case (mem_size)
        2'd0: begin // Byte
          gen_byte_en = 4'b0001 << calc_addr[1:0];
          gen_wdata   = {4{wdata_word0[7:0]}};
        end
        2'd1: begin // Short
          gen_byte_en = calc_addr[1] ? 4'b1100 : 4'b0011;
          gen_wdata   = {2{wdata_word0[15:0]}};
        end
        default: gen_byte_en = 4'b1111;
      endcase
    end
  end

  // Data extraction with sign/zero extension
  function automatic logic [31:0] format_rdata(
    input logic [31:0] raw,
    input logic [1:0]  size,
    input logic [1:0]  offset,
    input logic        sext
  );
    logic [7:0] b;
    logic [15:0] s;
    begin
      case (size)
        2'd0: begin
          case (offset)
            2'b00: b = raw[7:0];
            2'b01: b = raw[15:8];
            2'b10: b = raw[23:16];
            2'b11: b = raw[31:24];
          endcase
          return sext ? {{24{b[7]}}, b} : {24'h0, b};
        end
        2'd1: begin
          s = offset[1] ? raw[31:16] : raw[15:0];
          return sext ? {{16{s[15]}}, s} : {16'h0, s};
        end
        default: return raw;
      endcase
    end
  endfunction

  always_comb begin
    state_d          = state_q;
    burst_cnt_d      = burst_cnt_q;
    lsu_done         = 1'b0;
    bus_req.valid    = 1'b0;
    bus_req.write_en = is_store;
    bus_req.addr     = (calc_addr & ~32'h3) + {28'h0, burst_cnt_q, 2'b00};
    bus_req.wdata    = gen_wdata;
    bus_req.byte_en  = gen_byte_en;
    bus_req.size     = is_multi_word ? BUS_SIZE_BURST : bus_size_e'(mem_size);
    bus_req.burst_len= target_words;

    case (state_q)
      LSU_IDLE: begin
        if (req_valid && (is_load || is_store)) begin
          state_d       = LSU_BUS_ACTIVE;
          bus_req.valid = 1'b1;
          burst_cnt_d   = 2'd0;
        end
      end

      LSU_BUS_ACTIVE: begin
        bus_req.valid = 1'b1;
        if (bus_resp.ready) begin
          if (burst_cnt_q == target_words) begin
            state_d     = LSU_IDLE;
            lsu_done    = 1'b1;
            burst_cnt_d = 2'd0;
          end else begin
            burst_cnt_d = burst_cnt_q + 1'b1;
          end
        end
      end

      default: state_d = LSU_IDLE;
    endcase
  end

  integer j;
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      state_q     <= LSU_IDLE;
      burst_cnt_q <= 2'd0;
      for (j = 0; j < 4; j = j + 1) rdata_reg[j] <= 32'h0;
    end else begin
      state_q     <= state_d;
      burst_cnt_q <= burst_cnt_d;
      if (state_q == LSU_BUS_ACTIVE && bus_resp.ready && is_load) begin
        if (is_multi_word) begin
          rdata_reg[burst_cnt_q] <= bus_resp.rdata;
        end else begin
          rdata_reg[0] <= format_rdata(bus_resp.rdata, mem_size, calc_addr[1:0], mem_sign_extend);
        end
      end
    end
  end

endmodule : open960_lsu

`endif // OPEN960_LSU_SV

  always_comb begin
    case (scale_factor)
      3'd0: scaled_index = index_val;
      3'd1: scaled_index = index_val << 1;
      3'd2: scaled_index = index_val << 2;
      3'd3: scaled_index = index_val << 3;
      3'd4: scaled_index = index_val << 4;
      default: scaled_index = index_val;
    endcase
    calc_addr = base_val + scaled_index + disp_val;
  end

  assign effective_addr = calc_addr;
