// =============================================================================
// Open960 (RISC80960) - Instruction Decoder Unit
// =============================================================================

`ifndef OPEN960_DECODER_SV
`define OPEN960_DECODER_SV

`include "open960_pkg.sv"

module open960_decoder
  import open960_pkg::*;
(
  input  logic [31:0]    pc,
  input  logic [31:0]    instr_w0,
  input  logic [31:0]    instr_w1,
  
  output decoded_instr_t dec,
  output logic           is_64bit_instr
);

  logic [7:0] opcode;
  logic [6:0] op_ext;
  logic [4:0] dst;
  logic [4:0] src2;
  logic [4:0] src1;
  logic       m1, m2, m3;
  logic       mode_memb;

  assign opcode    = instr_w0[31:24];
  assign dst       = instr_w0[23:19];
  assign src2      = instr_w0[18:14];
  assign src1      = instr_w0[13:9];
  assign m3        = instr_w0[7];
  assign m2        = instr_w0[6];
  assign m1        = instr_w0[5];
  assign op_ext    = instr_w0[6:0];
  assign mode_memb = (opcode[7:4] >= 4'h8 && opcode[7:4] <= 4'hC) && instr_w0[13];
  assign is_64bit_instr = mode_memb;

  // Sign extension helpers
  function automatic logic [31:0] sign_ext24(input logic [23:0] in);
    return {{8{in[23]}}, in};
  endfunction

  function automatic logic [31:0] sign_ext13(input logic [12:0] in);
    return {{19{in[12]}}, in};
  endfunction

  always_comb begin
    dec                 = '0;
    dec.pc              = pc;
    dec.next_pc         = pc + (mode_memb ? 32'd8 : 32'd4);
    dec.opcode          = opcode;
    dec.opcode_ext      = op_ext;
    dec.dst_reg         = dst;
    dec.src1_reg        = src1;
    dec.src2_reg        = src2;
    dec.src3_reg        = 5'd0;
    dec.alu_op          = ALU_NOP;

    if (opcode <= 8'h1F) begin
      dec.fmt           = FMT_CTRL;
      dec.branch_target = pc + (sign_ext24(instr_w0[23:0]) << 2);
      case (opcode)
        OP_CTRL_B:    begin dec.is_branch = 1'b1; end
        OP_CTRL_CALL: begin dec.is_call   = 1'b1; dec.is_branch = 1'b1; end
        OP_CTRL_RET:  begin dec.is_ret    = 1'b1; dec.is_branch = 1'b1; end
        OP_CTRL_BAL:  begin dec.is_bal    = 1'b1; dec.is_branch = 1'b1; dec.dst_reg = 5'd30; dec.reg_write_en = 1'b1; dec.alu_op = ALU_PASSA; dec.imm_val = dec.next_pc; end
        default:      begin dec.is_branch = 1'b1; end
      endcase
    end else if (opcode >= 8'h20 && opcode <= 8'h3F) begin
      dec.fmt           = FMT_COBR;
      dec.src2_reg      = instr_w0[23:19];
      dec.src1_reg      = instr_w0[18:14];
      dec.branch_target = pc + (sign_ext13(instr_w0[12:0]) << 2);
      dec.is_branch     = 1'b1;
      if (instr_w0[13]) dec.imm_val = {27'h0, instr_w0[18:14]};
      case (opcode)
        OP_COBR_CMPBE, OP_COBR_CMPBNE, OP_COBR_CMPBL, OP_COBR_CMPBLE, OP_COBR_CMPBG, OP_COBR_CMPBGE: dec.alu_op = ALU_CMPO;
        OP_COBR_CMPIBE, OP_COBR_CMPIBNE, OP_COBR_CMPIBL, OP_COBR_CMPIBLE, OP_COBR_CMPIBG, OP_COBR_CMPIBGE: dec.alu_op = ALU_CMPI;
        OP_COBR_BBC, OP_COBR_BBS: dec.alu_op = ALU_CHKBIT;
        default: dec.alu_op = ALU_CMPO;
      endcase
    end else if (opcode >= 8'h80 && opcode <= 8'hCF) begin
      dec.fmt           = mode_memb ? FMT_MEMB : FMT_MEMA;
      dec.dst_reg       = dst;
      dec.src1_reg      = src2;
      if (mode_memb) begin
        dec.src3_reg    = instr_w0[9:5];
        dec.imm_val     = instr_w1;
      end else begin
        dec.imm_val     = instr_w0[12] ? (pc + sign_ext12(instr_w0[11:0])) : sign_ext12(instr_w0[11:0]);
      end
      case (opcode)
        OP_MEM_LD:   begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.mem_size = 2'd2; end
        OP_MEM_ST:   begin dec.is_store = 1'b1; dec.mem_size = 2'd2; end
        OP_MEM_LDOB: begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.mem_size = 2'd0; dec.mem_sign_extend = 1'b0; end
        OP_MEM_STOB: begin dec.is_store = 1'b1; dec.mem_size = 2'd0; end
    else begin
      dec.fmt           = FMT_REG;
      dec.dst_reg       = dst;
      dec.src2_reg      = src2;
      dec.src1_reg      = src1;
      dec.reg_write_en  = 1'b1;
      if (m1) dec.imm_val = {27'h0, src1};
      if (m2) dec.imm_val = {27'h0, src2};

      case (op_ext)
        OP_EXT_ADDO: dec.alu_op = ALU_ADD;
        OP_EXT_ADDI: begin dec.alu_op = ALU_ADD; dec.update_ac_flags = 1'b1; end
        OP_EXT_SUBO: dec.alu_op = ALU_SUB;
        OP_EXT_SUBI: begin dec.alu_op = ALU_SUB; dec.update_ac_flags = 1'b1; end
        OP_EXT_AND:  dec.alu_op = ALU_AND;
        OP_EXT_OR:   dec.alu_op = ALU_OR;
        OP_EXT_XOR:  dec.alu_op = ALU_XOR;
        OP_EXT_NOT:  dec.alu_op = ALU_NOT;
        OP_EXT_NOR:  dec.alu_op = ALU_NOR;
        OP_EXT_NAND: dec.alu_op = ALU_NAND;
        OP_EXT_XNOR: dec.alu_op = ALU_XNOR;
        OP_EXT_ANDNOT: dec.alu_op = ALU_ANDNOT;
        OP_EXT_NOTAND: dec.alu_op = ALU_NOTAND;
        OP_EXT_ORNOT:  dec.alu_op = ALU_ORNOT;
        OP_EXT_NOTOR:  dec.alu_op = ALU_NOTOR;
        OP_EXT_SHLO: dec.alu_op = ALU_SHL;
        OP_EXT_SHRO: dec.alu_op = ALU_SHR_LOGICAL;
        OP_EXT_SHLI: dec.alu_op = ALU_SHL;
        OP_EXT_SHRI: dec.alu_op = ALU_SHR_ARITH;
        OP_EXT_ROTATE: dec.alu_op = ALU_ROTATE;
        OP_EXT_SETBIT: dec.alu_op = ALU_SETBIT;
        OP_EXT_CLRBIT: dec.alu_op = ALU_CLRBIT;
        OP_EXT_NOTBIT: dec.alu_op = ALU_NOTBIT;
        OP_EXT_ALTERBIT: dec.alu_op = ALU_ALTERBIT;
        OP_EXT_CHKBIT: begin dec.alu_op = ALU_CHKBIT; dec.reg_write_en = 1'b0; dec.update_ac_cc = 1'b1; end
        OP_EXT_CMPO:   begin dec.alu_op = ALU_CMPO; dec.reg_write_en = 1'b0; dec.update_ac_cc = 1'b1; end
        OP_EXT_CMPI:   begin dec.alu_op = ALU_CMPI; dec.reg_write_en = 1'b0; dec.update_ac_cc = 1'b1; end
        OP_EXT_EXTRACT: dec.alu_op = ALU_EXTRACT;
        OP_EXT_MODIFY:  dec.alu_op = ALU_MODIFY;
        OP_EXT_SCANBIT: dec.alu_op = ALU_SCANBIT;
        OP_EXT_SPANBIT: dec.alu_op = ALU_SPANBIT;
        OP_EXT_MULO: dec.alu_op = ALU_MULO;
        OP_EXT_MULI: dec.alu_op = ALU_MULI;
        OP_EXT_DIVO: dec.alu_op = ALU_DIVO;
        OP_EXT_DIVI: dec.alu_op = ALU_DIVI;
        OP_EXT_REMO: dec.alu_op = ALU_REMO;
        OP_EXT_REMI: dec.alu_op = ALU_REMI;
        OP_EXT_EMUL: begin dec.alu_op = ALU_EMUL; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd1; end
        OP_EXT_EDIV: begin dec.alu_op = ALU_EDIV; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd1; end
        OP_EXT_MOV:  begin dec.alu_op = ALU_PASSA; end
        OP_EXT_MOVL: begin dec.alu_op = ALU_PASSA; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd1; end
        OP_EXT_MOVT: begin dec.alu_op = ALU_PASSA; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd2; end
        OP_EXT_MOVQ: begin dec.alu_op = ALU_PASSA; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd3; end
        OP_EXT_MODAC, OP_EXT_MODPC, OP_EXT_FLUSHREG: dec.is_sys_ctrl = 1'b1;
        default: ;
      endcase
    end
  end

endmodule : open960_decoder

`endif // OPEN960_DECODER_SV

        OP_MEM_LDOS: begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.mem_size = 2'd1; dec.mem_sign_extend = 1'b0; end
        OP_MEM_STOS: begin dec.is_store = 1'b1; dec.mem_size = 2'd1; end
        OP_MEM_LDIB: begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.mem_size = 2'd0; dec.mem_sign_extend = 1'b1; end
        OP_MEM_STIB: begin dec.is_store = 1'b1; dec.mem_size = 2'd0; end
        OP_MEM_LDIS: begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.mem_size = 2'd1; dec.mem_sign_extend = 1'b1; end
        OP_MEM_STIS: begin dec.is_store = 1'b1; dec.mem_size = 2'd1; end
        OP_MEM_LDA:  begin dec.reg_write_en = 1'b1; dec.alu_op = ALU_ADD; end
        OP_MEM_LDL:  begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd1; end
        OP_MEM_STL:  begin dec.is_store = 1'b1; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd1; end
        OP_MEM_LDT:  begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd2; end
        OP_MEM_STT:  begin dec.is_store = 1'b1; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd2; end
        OP_MEM_LDQ:  begin dec.is_load = 1'b1; dec.reg_write_en = 1'b1; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd3; end
        OP_MEM_STQ:  begin dec.is_store = 1'b1; dec.is_multi_word = 1'b1; dec.multi_word_count = 2'd3; end
        default: ;
      endcase
    end

  function automatic logic [31:0] sign_ext12(input logic [11:0] in);
    return {{20{in[11]}}, in};
  endfunction
