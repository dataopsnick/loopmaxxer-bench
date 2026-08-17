// =============================================================================
// Open960 (RISC80960) - Multiplier / Divider Unit (MULDIV)
// =============================================================================

`ifndef OPEN960_MULDIV_SV
`define OPEN960_MULDIV_SV

`include "open960_pkg.sv"

module open960_muldiv
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  input  logic        start,
  input  alu_op_e     op,
  input  logic [31:0] op_a,      // Divisor or Multiplier
  input  logic [31:0] op_b,      // Dividend low or Multiplicand
  input  logic [31:0] op_b_high, // Dividend high (for ediv)

  output logic        busy,
  output logic        done,
  output logic [31:0] res_low,
  output logic [31:0] res_high
);

  logic [63:0] mul_res_u;
  logic signed [63:0] mul_res_s;
  logic [5:0]  div_cnt;
  logic [63:0] dividend_q;
  logic [31:0] divisor_q;
  logic [31:0] quot_q;
  logic        is_signed_div;
  logic        sign_quot;
  logic        sign_rem;
  logic        running_div;

  assign mul_res_u = 64'(op_b) * 64'(op_a);
  assign mul_res_s = 64'($signed(op_b)) * 64'($signed(op_a));

  assign busy = running_div;

  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      running_div   <= 1'b0;
      div_cnt       <= '0;
      dividend_q    <= '0;
      divisor_q     <= '0;
      quot_q        <= '0;
      done          <= 1'b0;
      res_low       <= '0;
      res_high      <= '0;
      is_signed_div <= 1'b0;
      sign_quot     <= 1'b0;
      sign_rem      <= 1'b0;
    end else begin
      done <= 1'b0;

      if (start && !running_div) begin
        case (op)
          ALU_MULO: begin
            res_low  <= mul_res_u[31:0];
            res_high <= mul_res_u[63:32];
            done     <= 1'b1;
          end
          ALU_MULI: begin
            res_low  <= mul_res_s[31:0];
            res_high <= mul_res_s[63:32];
            done     <= 1'b1;
          end
          ALU_EMUL: begin
            res_low  <= mul_res_u[31:0];
            res_high <= mul_res_u[63:32];
            done     <= 1'b1;
          end
          ALU_DIVO, ALU_REMO: begin
            if (op_a == 32'h0) begin // Division by zero
              res_low  <= 32'hFFFF_FFFF;
              res_high <= op_b;
              done     <= 1'b1;
            end else begin
              running_div   <= 1'b1;
              div_cnt       <= 6'd32;
              dividend_q    <= {32'h0, op_b};
              divisor_q     <= op_a;
              quot_q        <= 32'h0;
              is_signed_div <= 1'b0;
            end
          end
          ALU_DIVI, ALU_REMI: begin
            if (op_a == 32'h0) begin
              res_low  <= 32'hFFFF_FFFF;
              res_high <= op_b;
              done     <= 1'b1;
            end else begin
              running_div   <= 1'b1;
              div_cnt       <= 6'd32;
              sign_quot     <= op_b[31] ^ op_a[31];
              sign_rem      <= op_b[31];
              dividend_q    <= {32'h0, op_b[31] ? -op_b : op_b};
              divisor_q     <= op_a[31] ? -op_a : op_a;
              quot_q        <= 32'h0;
              is_signed_div <= 1'b1;
            end
          end
          default: ;
        endcase
      end else if (running_div) begin
        // Non-restoring / Restoring iterative divider step
        logic [63:0] next_div;
        next_div = {dividend_q[62:0], 1'b0};
        if (next_div[63:32] >= divisor_q) begin
          dividend_q <= {next_div[63:32] - divisor_q, next_div[31:0] | 32'h1};
        end else begin
          dividend_q <= next_div;
        end

        div_cnt <= div_cnt - 1'b1;
        if (div_cnt == 6'd1) begin
          running_div <= 1'b0;
          done        <= 1'b1;
          if (is_signed_div) begin
            res_low  <= sign_quot ? -dividend_q[31:0] : dividend_q[31:0];
            res_high <= sign_rem  ? -dividend_q[63:32] : dividend_q[63:32];
          end else begin
            res_low  <= dividend_q[31:0];
            res_high <= dividend_q[63:32];
          end
        end
      end
    end
  end

endmodule : open960_muldiv

`endif // OPEN960_MULDIV_SV
