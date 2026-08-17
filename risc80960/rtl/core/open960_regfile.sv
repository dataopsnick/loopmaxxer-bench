// =============================================================================
// Open960 (RISC80960) - Dual-Bank Register File (Global g0-g15 & Local r0-r15)
// =============================================================================

`ifndef OPEN960_REGFILE_SV
`define OPEN960_REGFILE_SV

`include "open960_pkg.sv"

module open960_regfile
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Read Port 1
  input  logic [4:0]  raddr1,
  output logic [31:0] rdata1,

  // Read Port 2
  input  logic [4:0]  raddr2,
  output logic [31:0] rdata2,

  // Read Port 3 (Used for 3rd operand / base-index / multi-word)
  input  logic [4:0]  raddr3,
  output logic [31:0] rdata3,

  // Pipeline Writeback Port
  input  logic        we,
  input  logic [4:0]  waddr,
  input  logic [31:0] wdata,
  input  logic        we_word1,
  input  logic [4:0]  waddr_word1,
  input  logic [31:0] wdata_word1,
  input  logic        we_word2,
  input  logic [4:0]  waddr_word2,
  input  logic [31:0] wdata_word2,
  input  logic        we_word3,
  input  logic [4:0]  waddr_word3,
  input  logic [31:0] wdata_word3,

  // Frame Cache Interface (Direct Local Register Access)
  input  logic        frame_restore_we,
  input  logic [3:0]  frame_restore_reg,
  input  logic [31:0] frame_restore_data,
  input  logic [3:0]  frame_spill_reg,
  output logic [31:0] frame_spill_data
);

  // Global Registers (g0 - g15) -> index [16:31]
  logic [31:0] global_regs [0:15];
  // Local Registers (r0 - r15) -> index [0:15]
  logic [31:0] local_regs  [0:15];

  // Helper read function
  function automatic logic [31:0] read_reg(input logic [4:0] addr);
    if (addr[4]) begin
      return global_regs[addr[3:0]];
    end else begin
      return local_regs[addr[3:0]];
    end
  endfunction

  // Read Ports
  assign rdata1 = read_reg(raddr1);
  assign rdata2 = read_reg(raddr2);
  assign rdata3 = read_reg(raddr3);

  // Frame spill port read
  assign frame_spill_data = local_regs[frame_spill_reg];

  // Synchronous Write Logic with bypass
  integer i;
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      for (i = 0; i < 16; i = i + 1) begin
        global_regs[i] <= 32'h0;
        local_regs[i]  <= 32'h0;
      end
    end else begin
      // Frame cache bulk restore has priority on local regs
      if (frame_restore_we) begin
        local_regs[frame_restore_reg] <= frame_restore_data;
      end

      // Standard pipeline writeback
      if (we) begin
        if (waddr[4]) begin
          global_regs[waddr[3:0]] <= wdata;
        end else if (!frame_restore_we || frame_restore_reg != waddr[3:0]) begin
          local_regs[waddr[3:0]] <= wdata;
        end
      end

      // Multi-word writes (word 1)
      if (we_word1) begin
        if (waddr_word1[4]) begin
          global_regs[waddr_word1[3:0]] <= wdata_word1;
        end else if (!frame_restore_we || frame_restore_reg != waddr_word1[3:0]) begin
          local_regs[waddr_word1[3:0]] <= wdata_word1;
        end
      end

      // Multi-word writes (word 2)
      if (we_word2) begin
        if (waddr_word2[4]) begin
          global_regs[waddr_word2[3:0]] <= wdata_word2;
        end else if (!frame_restore_we || frame_restore_reg != waddr_word2[3:0]) begin
          local_regs[waddr_word2[3:0]] <= wdata_word2;
        end
      end

      // Multi-word writes (word 3)
      if (we_word3) begin
        if (waddr_word3[4]) begin
          global_regs[waddr_word3[3:0]] <= wdata_word3;
        end else if (!frame_restore_we || frame_restore_reg != waddr_word3[3:0]) begin
          local_regs[waddr_word3[3:0]] <= wdata_word3;
        end
      end
    end
  end

endmodule : open960_regfile

`endif // OPEN960_REGFILE_SV
