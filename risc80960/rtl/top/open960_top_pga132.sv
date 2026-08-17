// =============================================================================
// Open960 (RISC80960) - 132-Pin PGA Drop-In Package Wrapper (i960KA/KB)
// =============================================================================

`ifndef OPEN960_TOP_PGA132_SV
`define OPEN960_TOP_PGA132_SV

`include "open960_pkg.sv"
`include "open960_core.sv"
`include "open960_lbus_biu.sv"

module open960_top_pga132
  import open960_pkg::*;
(
  // System Clock & Reset
  input  logic        CLK2,       // 2x System Clock Input
  input  logic        RESET_N,    // Active Low System Reset

  // Multiplexed Local Address/Data Bus
  inout  wire  [31:0] LAD,        // LAD[31:0]

  // Bus Control & Status Signals (Active Low)
  output logic [3:0]  BE_N,       // BE3#, BE2#, BE1#, BE0# (Byte Enables)
  output logic        ADS_N,      // Address Strobe
  input  logic        READY_N,    // Bus Ready input
  output logic        BLAST_N,    // Burst Last indicator
  output logic        WR_N,       // Write / Read (1=Write, 0=Read)
  output logic        DEN_N,      // Data Enable
  output logic        DTR_N,      // Data Transmit / Receive
  output logic        LOCK_N,     // Bus Lock

  // Interrupt Inputs
  input  logic [3:0]  INT_N,      // INT0#, INT1#, INT2#, INT3#
  input  logic        NMI_N,      // Non-Maskable Interrupt

  // DMA / Bus Arbitration
  input  logic        HOLD,       // Bus Hold Request
  output logic        HLDA,       // Bus Hold Acknowledge

  // Self-Test & Status
  output logic        FAIL_N      // Self-Test Failure indicator
);

  // Divide CLK2 by 2 to generate internal system clock CLK1
  logic clk_core;
  always_ff @(posedge CLK2 or negedge RESET_N) begin
    if (!RESET_N) begin
      clk_core <= 1'b0;
    end else begin
      clk_core <= ~clk_core;
    end
  end

  assign LOCK_N = 1'b1;
  assign HLDA   = 1'b0;
  assign FAIL_N = 1'b1; // Pass self-test

  // Internal Core & BIU Interconnect
  bus_req_t  core_bus_req;
  bus_resp_t core_bus_resp;

  open960_core u_core (
    .clk       (clk_core),
    .rst_n     (RESET_N),
    .m_bus_req (core_bus_req),
    .m_bus_resp(core_bus_resp),
    .ext_intr_n(INT_N),
    .ext_nmi_n (NMI_N)
  );

  open960_lbus_biu u_biu (
    .clk      (clk_core),
    .rst_n    (RESET_N),
    .core_req (core_bus_req),
    .core_resp(core_bus_resp),
    .lad      (LAD),
    .be_n     (BE_N),
    .ads_n    (ADS_N),
    .ready_n  (READY_N),
    .blast_n  (BLAST_N),
    .w_r_n    (WR_N),
    .den_n    (DEN_N),
    .dt_r_n   (DTR_N)
  );

endmodule : open960_top_pga132

`endif // OPEN960_TOP_PGA132_SV
