// =============================================================================
// Open960 (RISC80960) - AXI4-Lite Master BIU
// =============================================================================

`ifndef OPEN960_AXI_BIU_SV
`define OPEN960_AXI_BIU_SV

`include "open960_pkg.sv"

module open960_axi_biu
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // Core internal interface
  input  bus_req_t    core_req,
  output bus_resp_t   core_resp,

  // AXI4-Lite Write Address Channel
  output logic [31:0] m_axi_awaddr,
  output logic [2:0]  m_axi_awprot,
  output logic        m_axi_awvalid,
  input  logic        m_axi_awready,

  // AXI4-Lite Write Data Channel
  output logic [31:0] m_axi_wdata,
  output logic [3:0]  m_axi_wstrb,
  output logic        m_axi_wvalid,
  input  logic        m_axi_wready,

  // AXI4-Lite Write Response Channel
  input  logic [1:0]  m_axi_bresp,
  input  logic        m_axi_bvalid,
  output logic        m_axi_bready,

  // AXI4-Lite Read Address Channel
  output logic [31:0] m_axi_araddr,
  output logic [2:0]  m_axi_arprot,
  output logic        m_axi_arvalid,
  input  logic        m_axi_arready,

  // AXI4-Lite Read Data Channel
  input  logic [31:0] m_axi_rdata,
  input  logic [1:0]  m_axi_rresp,
  input  logic        m_axi_rvalid,
  output logic        m_axi_rready
);

  typedef enum logic [2:0] {
    AXI_IDLE,
    AXI_WR_ADDR_DATA,
    AXI_WR_RESP,
    AXI_RD_ADDR,
    AXI_RD_DATA
  } axi_state_e;

  axi_state_e state_q, state_d;
  logic [31:0] rdata_captured;

  assign m_axi_awprot = 3'b000;
  assign m_axi_arprot = 3'b000;
  assign m_axi_awaddr = core_req.addr;
  assign m_axi_araddr = core_req.addr;
  assign m_axi_wdata  = core_req.wdata;
  assign m_axi_wstrb  = core_req.byte_en;

  always_comb begin
    state_d         = state_q;
    m_axi_awvalid   = 1'b0;
    m_axi_wvalid    = 1'b0;
    m_axi_bready    = 1'b0;
    m_axi_arvalid   = 1'b0;
    m_axi_rready    = 1'b0;
    core_resp.ready = 1'b0;
    core_resp.rdata = rdata_captured;
    core_resp.error = 1'b0;

    case (state_q)
      AXI_IDLE: begin
        if (core_req.valid) begin
          if (core_req.write_en) begin
            state_d       = AXI_WR_ADDR_DATA;
            m_axi_awvalid = 1'b1;
            m_axi_wvalid  = 1'b1;
          end else begin
            state_d       = AXI_RD_ADDR;
            m_axi_arvalid = 1'b1;
          end
        end
      end

      AXI_WR_ADDR_DATA: begin
        m_axi_awvalid = 1'b1;
        m_axi_wvalid  = 1'b1;
        if (m_axi_awready && m_axi_wready) begin
          state_d      = AXI_WR_RESP;
          m_axi_bready = 1'b1;
        end
      end

      AXI_WR_RESP: begin
        m_axi_bready = 1'b1;
        if (m_axi_bvalid) begin
          core_resp.ready = 1'b1;
          core_resp.error = (m_axi_bresp != 2'b00);
          state_d         = AXI_IDLE;
        end
      end

      AXI_RD_ADDR: begin
        m_axi_arvalid = 1'b1;
        if (m_axi_arready) begin
          state_d      = AXI_RD_DATA;
          m_axi_rready = 1'b1;
        end
      end

      AXI_RD_DATA: begin
        m_axi_rready = 1'b1;
        if (m_axi_rvalid) begin
          core_resp.ready = 1'b1;
          core_resp.rdata = m_axi_rdata;
          core_resp.error = (m_axi_rresp != 2'b00);
          state_d         = AXI_IDLE;
        end
      end

      default: state_d = AXI_IDLE;
    endcase
  end

  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      state_q        <= AXI_IDLE;
      rdata_captured <= 32'h0;
    end else begin
      state_q <= state_d;
      if (state_q == AXI_RD_DATA && m_axi_rvalid) begin
        rdata_captured <= m_axi_rdata;
      end
    end
  end

endmodule : open960_axi_biu

`endif // OPEN960_AXI_BIU_SV
