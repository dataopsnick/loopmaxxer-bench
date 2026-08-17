// =============================================================================
// Open960 (RISC80960) - Local Register Window Frame Cache & Spill/Fill FSM
// =============================================================================

`ifndef OPEN960_FRAME_CACHE_SV
`define OPEN960_FRAME_CACHE_SV

`include "open960_pkg.sv"

module open960_frame_cache
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  input  logic        cmd_call,
  input  logic        cmd_ret,
  input  logic        cmd_flush,
  input  logic [31:0] curr_pfp,
  input  logic [31:0] curr_sp,
  input  logic [31:0] curr_rip,
  input  logic [31:0] curr_fp,

  output logic        fsm_busy,
  output logic [31:0] new_pfp,
  output logic [31:0] new_sp,
  output logic [31:0] new_rip,
  output logic [31:0] new_fp,
  output logic        new_frame_init_en,

  output logic        rf_restore_we,
  output logic [3:0]  rf_restore_reg,
  output logic [31:0] rf_restore_data,
  output logic [3:0]  rf_spill_reg,
  input  logic [31:0] rf_spill_data,

  output logic        mem_req_valid,
  output logic        mem_req_write,
  output logic [31:0] mem_req_addr,
  output logic [31:0] mem_req_wdata,
  input  logic        mem_resp_ready,
  input  logic [31:0] mem_resp_rdata
);

  localparam int CACHE_FRAMES = 4;
  logic [31:0] frame_mem [0:CACHE_FRAMES-1][0:15];
  logic [31:0] frame_fp_tag [0:CACHE_FRAMES-1];
  logic [$clog2(CACHE_FRAMES)-1:0] head_ptr;
  logic [$clog2(CACHE_FRAMES):0] count;

  typedef enum logic [2:0] {
    ST_IDLE, ST_CALL_ALLOC, ST_SPILL_BURST, ST_RET_RESTORE, ST_FILL_BURST
  } state_e;

  state_e state_q, state_d;
  always_comb begin
    state_d            = state_q;
    word_idx_d         = word_idx_q;
    spill_base_addr_d  = spill_base_addr_q;
    fill_base_addr_d   = fill_base_addr_q;
    mem_req_valid      = 1'b0;
    mem_req_write      = 1'b0;
    mem_req_addr       = 32'h0;
    mem_req_wdata      = 32'h0;
    rf_restore_we      = 1'b0;
    rf_restore_data    = 32'h0;
    new_frame_init_en  = 1'b0;
    new_pfp            = 32'h0;
    new_sp             = 32'h0;
    new_rip            = 32'h0;
    new_fp             = 32'h0;

    case (state_q)
      ST_IDLE: begin
        if (cmd_call) begin
          if (count == CACHE_FRAMES) begin
            state_d           = ST_SPILL_BURST;
            word_idx_d        = 4'd0;
            spill_base_addr_d = frame_fp_tag[(head_ptr - count[1:0]) % CACHE_FRAMES];
          end else begin
            state_d           = ST_CALL_ALLOC;
          end
        end else if (cmd_ret) begin
          if (count > 0) begin
            state_d           = ST_RET_RESTORE;
            word_idx_d        = 4'd0;
          end else begin
            state_d           = ST_FILL_BURST;
            word_idx_d        = 4'd0;
            fill_base_addr_d  = curr_pfp & ~32'h3F;
          end
        end
      end

      ST_CALL_ALLOC: begin
        new_frame_init_en = 1'b1;
        new_pfp           = curr_fp;
        new_sp            = align64(curr_sp == 32'h0 ? curr_fp + 32'd64 : curr_sp + 32'd64);
        new_rip           = curr_rip;
        new_fp            = (curr_sp == 32'h0) ? curr_fp + 32'd64 : align64(curr_sp);
        state_d           = ST_IDLE;
      end

      ST_SPILL_BURST: begin
        mem_req_valid = 1'b1;
        mem_req_write = 1'b1;
        mem_req_addr  = spill_base_addr_q + {26'h0, word_idx_q, 2'b00};
        mem_req_wdata = frame_mem[(head_ptr - count[1:0]) % CACHE_FRAMES][word_idx_q];
        if (mem_resp_ready) begin
          if (word_idx_q == 4'd15) begin
            state_d    = ST_CALL_ALLOC;
            word_idx_d = 4'd0;
          end else begin
            word_idx_d = word_idx_q + 1'b1;
          end
        end
      end

      ST_RET_RESTORE: begin
        rf_restore_we   = 1'b1;
        rf_restore_data = frame_mem[head_ptr - 1'b1][word_idx_q];
        if (word_idx_q == 4'd15) begin
          state_d    = ST_IDLE;
          word_idx_d = 4'd0;
        end else begin
          word_idx_d = word_idx_q + 1'b1;
        end
      end

      ST_FILL_BURST: begin
        mem_req_valid = 1'b1;
        mem_req_write = 1'b0;
        mem_req_addr  = fill_base_addr_q + {26'h0, word_idx_q, 2'b00};
        if (mem_resp_ready) begin
          rf_restore_we   = 1'b1;
          rf_restore_data = mem_resp_rdata;
          if (word_idx_q == 4'd15) begin
            state_d    = ST_IDLE;
            word_idx_d = 4'd0;
          end else begin
            word_idx_d = word_idx_q + 1'b1;
          end
        end
      end

      default: state_d = ST_IDLE;
    endcase
  end

  integer f, w;
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      state_q           <= ST_IDLE;
      word_idx_q        <= 4'd0;
      spill_base_addr_q <= 32'h0;
      fill_base_addr_q  <= 32'h0;
      head_ptr          <= '0;
      count             <= '0;
      for (f = 0; f < CACHE_FRAMES; f = f + 1) begin
        frame_fp_tag[f] <= 32'h0;
        for (w = 0; w < 16; w = w + 1) begin
          frame_mem[f][w] <= 32'h0;
        end
      end
    end else begin
      state_q           <= state_d;
      word_idx_q        <= word_idx_d;
      spill_base_addr_q <= spill_base_addr_d;
      fill_base_addr_q  <= fill_base_addr_d;

      if (state_q == ST_IDLE && cmd_call && count < CACHE_FRAMES) begin
        frame_fp_tag[head_ptr] <= curr_fp;
        head_ptr               <= head_ptr + 1'b1;
        count                  <= count + 1'b1;
      end else if (state_q == ST_RET_RESTORE && word_idx_q == 4'd15) begin
        head_ptr               <= head_ptr - 1'b1;
        count                  <= count - 1'b1;
      end
    end
  end

endmodule : open960_frame_cache

`endif // OPEN960_FRAME_CACHE_SV

  logic [3:0] word_idx_q, word_idx_d;
  logic [31:0] spill_base_addr_q, spill_base_addr_d;
  logic [31:0] fill_base_addr_q, fill_base_addr_d;

  assign fsm_busy = (state_q != ST_IDLE);
  assign rf_restore_reg = word_idx_q;
  assign rf_spill_reg   = word_idx_q;

  function automatic logic [31:0] align64(input logic [31:0] addr);
    return (addr + 32'd63) & ~32'd63;
  endfunction
