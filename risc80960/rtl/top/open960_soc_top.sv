// =============================================================================
// Open960 (RISC80960) - SoC Simulation & FPGA Prototyping Top Level
// =============================================================================

`ifndef OPEN960_SOC_TOP_SV
`define OPEN960_SOC_TOP_SV

`include "open960_pkg.sv"
`include "open960_core.sv"

module open960_soc_top
  import open960_pkg::*;
#(
  parameter int MEM_SIZE_WORDS = 16384 // 64 KB Internal RAM
)(
  input  logic        clk,
  input  logic        rst_n,

  // UART Interface
  output logic        uart_tx_out,
  output logic [7:0]  uart_tx_char,
  output logic        uart_tx_valid,

  // Testbench Status / Halt Monitoring
  output logic        sim_halted,
  output logic [31:0] sim_exit_code
);

  bus_req_t  core_req;
  bus_resp_t core_resp;

  // 64 KB Memory Array
  logic [31:0] ram [0:MEM_SIZE_WORDS-1];
  logic [31:0] cycle_cnt;

  open960_core u_core (
    .clk       (clk),
    .rst_n     (rst_n),
    .m_bus_req (core_req),
    .m_bus_resp(core_resp),
    .ext_intr_n(4'b1111),
    .ext_nmi_n (1'b1)
  );

  // Cycle counter
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      cycle_cnt <= 32'h0;
    end else begin
      cycle_cnt <= cycle_cnt + 1'b1;
    end
  end

  // Memory & Peripheral Bus Responder
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      core_resp.ready <= 1'b0;
      core_resp.rdata <= 32'h0;
      core_resp.error <= 1'b0;
      uart_tx_valid   <= 1'b0;
      uart_tx_char    <= 8'h0;
      uart_tx_out     <= 1'b1;
      sim_halted      <= 1'b0;
      sim_exit_code   <= 32'h0;
    end else begin
      core_resp.ready <= 1'b0;
      uart_tx_valid   <= 1'b0;

      if (core_req.valid && !core_resp.ready) begin
        core_resp.ready <= 1'b1;

        if (core_req.addr[31:24] == 8'hFF) begin
          // Memory-mapped Peripherals
          if (core_req.addr[7:0] == 8'h00) begin
            // UART TX Data Register
            if (core_req.write_en) begin
              uart_tx_char  <= core_req.wdata[7:0];
              uart_tx_valid <= 1'b1;
            end else begin
              core_resp.rdata <= 32'h0;
            end
          end else if (core_req.addr[7:0] == 8'h04) begin
            // UART Status Register (bit 0 = TX Ready)
            core_resp.rdata <= 32'h0000_0001;
          end else if (core_req.addr[7:0] == 8'h10) begin
            // Cycle Counter
            core_resp.rdata <= cycle_cnt;
          end else if (core_req.addr[7:0] == 8'hF0) begin
            // Simulation Exit / Halt Register
            if (core_req.write_en) begin
              sim_halted    <= 1'b1;
              sim_exit_code <= core_req.wdata;
            end
          end
        end else begin
          // RAM Access
          logic [13:0] word_idx;
          word_idx = core_req.addr[15:2];

          if (core_req.write_en) begin
            if (core_req.byte_en[0]) ram[word_idx][7:0]   <= core_req.wdata[7:0];
            if (core_req.byte_en[1]) ram[word_idx][15:8]  <= core_req.wdata[15:8];
            if (core_req.byte_en[2]) ram[word_idx][23:16] <= core_req.wdata[23:16];
            if (core_req.byte_en[3]) ram[word_idx][31:24] <= core_req.wdata[31:24];
          end else begin
            core_resp.rdata <= ram[word_idx];
          end
        end
      end
    end
  end

endmodule : open960_soc_top

`endif // OPEN960_SOC_TOP_SV
