// =============================================================================
// Open960 (RISC80960) - 32-bit Execution Arithmetic Logic Unit (ALU)
// =============================================================================

`ifndef OPEN960_ALU_SV
`define OPEN960_ALU_SV

`include "open960_pkg.sv"

module open960_alu
  import open960_pkg::*;
(
  input  alu_op_e     alu_op,
  input  logic [31:0] op_a,           // Typically src1 / literal
  input  logic [31:0] op_b,           // Typically src2
  input  logic [31:0] op_c,           // Optional 3rd operand (e.g. length for extract)
  input  logic [2:0]  ac_cc_in,       // Current condition code {cc_g, cc_e, cc_l}
  
  output logic [31:0] alu_result,
  output logic [2:0]  ac_cc_out,      // Updated {cc_g, cc_e, cc_l}
  output logic        int_overflow
);

  logic [4:0]  shift_amt;
  logic [32:0] add_ext;
  logic [32:0] sub_ext;
  logic        signed_a_lt_b;
  logic        signed_a_gt_b;
  logic        signed_a_eq_b;
  logic        unsigned_a_lt_b;
  logic        unsigned_a_gt_b;
  logic        unsigned_a_eq_b;

  assign shift_amt = op_a[4:0];

  // Arithmetic intermediate sums
  assign add_ext = {1'b0, op_b} + {1'b0, op_a};
  assign sub_ext = {1'b0, op_b} - {1'b0, op_a};

  // Signed / Unsigned Comparison Logic (src1 vs src2 -> op_a vs op_b)
  assign signed_a_eq_b   = ($signed(op_a) == $signed(op_b));
  assign signed_a_lt_b   = ($signed(op_a) <  $signed(op_b));
  assign signed_a_gt_b   = ($signed(op_a) >  $signed(op_b));

  assign unsigned_a_eq_b = (op_a == op_b);
  assign unsigned_a_lt_b = (op_a <  op_b);
  assign unsigned_a_gt_b = (op_a >  op_b);

  // Leading bit scanner
  function automatic logic [31:0] scan_msb(input logic [31:0] val, input logic target_bit);
    integer i;
    logic found;
    logic [4:0] pos;
    begin
      found = 1'b0;
  always_comb begin
    alu_result   = 32'h0;
    ac_cc_out    = ac_cc_in;
    int_overflow = 1'b0;

    case (alu_op)
      ALU_ADD: begin
        alu_result   = op_b + op_a;
        // Signed overflow: (op_b[31] == op_a[31]) && (alu_result[31] != op_b[31])
        int_overflow = (op_b[31] == op_a[31]) && (alu_result[31] != op_b[31]);
      end

      ALU_SUB: begin
        alu_result   = op_b - op_a;
        // Signed overflow: (op_b[31] != op_a[31]) && (alu_result[31] != op_b[31])
        int_overflow = (op_b[31] != op_a[31]) && (alu_result[31] != op_b[31]);
      end

      ALU_AND:     alu_result = op_b & op_a;
      ALU_OR:      alu_result = op_b | op_a;
      ALU_XOR:     alu_result = op_b ^ op_a;
      ALU_NOT:     alu_result = ~op_a;
      ALU_NOR:     alu_result = ~(op_b | op_a);
      ALU_NAND:    alu_result = ~(op_b & op_a);
      ALU_XNOR:    alu_result = ~(op_b ^ op_a);
      ALU_ANDNOT:  alu_result = op_b & ~op_a;
      ALU_NOTAND:  alu_result = ~op_b & op_a;
      ALU_ORNOT:   alu_result = op_b | ~op_a;
      ALU_NOTOR:   alu_result = ~op_b | op_a;

      ALU_SHL:         alu_result = op_b << shift_amt;
      ALU_SHR_LOGICAL: alu_result = op_b >> shift_amt;
      ALU_SHR_ARITH:   alu_result = $signed(op_b) >>> shift_amt;
      ALU_ROTATE:      alu_result = (op_b << shift_amt) | (op_b >> (32 - shift_amt));

      ALU_SETBIT:   alu_result = op_b | (32'h1 << shift_amt);
      ALU_CLRBIT:   alu_result = op_b & ~(32'h1 << shift_amt);
      ALU_NOTBIT:   alu_result = op_b ^ (32'h1 << shift_amt);
      ALU_ALTERBIT: alu_result = ac_cc_in[1] ? (op_b | (32'h1 << shift_amt)) : (op_b & ~(32'h1 << shift_amt));

      ALU_CHKBIT: begin
        alu_result = 32'h0;
        // If bit is 0 -> CC_E (3'b010), else CC = 3'b000
        ac_cc_out  = (op_b[shift_amt] == 1'b0) ? 3'b010 : 3'b000;
      end

      ALU_CMPO: begin
        alu_result = 32'h0;
        if (unsigned_a_eq_b)      ac_cc_out = 3'b010; // Equal
        else if (unsigned_a_gt_b) ac_cc_out = 3'b100; // Greater (op_a > op_b)
        else                      ac_cc_out = 3'b001; // Less (op_a < op_b)
      end

      ALU_CMPI: begin
        alu_result = 32'h0;
        if (signed_a_eq_b)      ac_cc_out = 3'b010; // Equal
        else if (signed_a_gt_b) ac_cc_out = 3'b100; // Greater
        else                    ac_cc_out = 3'b001; // Less
      end

      ALU_EXTRACT: begin
        // op_a: bit offset, op_c: length (5-bit), op_b: source word
        logic [4:0] len;
        logic [63:0] shifted;
        len = op_c[4:0];
        shifted = {32'h0, op_b} >> op_a[4:0];
        alu_result = shifted[31:0] & ((len == 5'd0) ? 32'hFFFF_FFFF : ((32'h1 << len) - 1'b1));
      end

      ALU_MODIFY: begin
        // op_a: mask, op_b: source, op_c: current destination
        alu_result = (op_c & ~op_a) | (op_b & op_a);
      end

      ALU_SCANBIT: alu_result = scan_msb(op_a, 1'b1);
      ALU_SPANBIT: alu_result = scan_msb(op_a, 1'b0);

      ALU_PASSA: alu_result = op_a;
      ALU_PASSB: alu_result = op_b;

      default: alu_result = 32'h0;
    endcase
  end

endmodule : open960_alu

`endif // OPEN960_ALU_SV

      pos = 5'd0;
      for (i = 31; i >= 0; i = i - 1) begin
        if (!found && val[i] == target_bit) begin
          found = 1'b1;
          pos = i[4:0];
        end
      end
      return found ? {27'h0, pos} : 32'hFFFF_FFFF;
    end
  endfunction
