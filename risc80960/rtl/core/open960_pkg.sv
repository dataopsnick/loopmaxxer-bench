// =============================================================================
// Open960 (RISC80960) - Clean-Room Intel 80960 Architecture Definition Package
// Part 1: Core Parameters, Register Maps, and Formats
// =============================================================================

`ifndef OPEN960_PKG_SV
`define OPEN960_PKG_SV

package open960_pkg;

  localparam int XLEN               = 32;
  localparam int REG_ADDR_WIDTH     = 5;
  localparam int NUM_GLOBAL_REGS    = 16;
  localparam int NUM_LOCAL_REGS     = 16;
  localparam int FRAME_CACHE_DEPTH  = 8;
  localparam int RESET_VECTOR       = 32'h0000_0000;
  localparam int FAULT_TABLE_BASE   = 32'h0000_0400;
  localparam int INTR_TABLE_BASE    = 32'h0000_0800;

  localparam bit [4:0] REG_PFP      = 5'd0;   // r0: Previous Frame Pointer
  localparam bit [4:0] REG_SP       = 5'd1;   // r1: Stack Pointer
  localparam bit [4:0] REG_RIP      = 5'd2;   // r2: Return IP
  localparam bit [4:0] REG_FP       = 5'd31;  // g15: Frame Pointer

  typedef enum logic [2:0] {
    FMT_CTRL, FMT_COBR, FMT_MEMA, FMT_MEMB, FMT_REG
  } instr_fmt_e;

  // Primary Opcodes
  localparam bit [7:0] OP_CTRL_CALL      = 8'h09;
  localparam bit [7:0] OP_CTRL_RET       = 8'h0A;
  localparam bit [7:0] OP_CTRL_BAL       = 8'h0B;
  localparam bit [7:0] OP_CTRL_B         = 8'h10;
  localparam bit [7:0] OP_CTRL_BNO       = 8'h11;
  localparam bit [7:0] OP_CTRL_BG        = 8'h12;
  localparam bit [7:0] OP_CTRL_BE        = 8'h13;
  localparam bit [7:0] OP_CTRL_BGE       = 8'h14;
  localparam bit [7:0] OP_CTRL_BL        = 8'h15;
  localparam bit [7:0] OP_CTRL_BNE       = 8'h16;
  localparam bit [7:0] OP_CTRL_BLE       = 8'h17;
  localparam bit [7:0] OP_CTRL_BO        = 8'h18;
  localparam bit [7:0] OP_CTRL_FAULT     = 8'h1A;

  localparam bit [7:0] OP_COBR_CMPBNO    = 8'h21;
  localparam bit [7:0] OP_COBR_CMPBG     = 8'h22;
  localparam bit [7:0] OP_COBR_CMPBE     = 8'h23;
  localparam bit [7:0] OP_COBR_CMPBGE    = 8'h24;
  localparam bit [7:0] OP_COBR_CMPBL     = 8'h25;
  localparam bit [7:0] OP_COBR_CMPBNE    = 8'h26;
  localparam bit [7:0] OP_COBR_CMPBLE    = 8'h27;
  localparam bit [7:0] OP_COBR_CMPBO     = 8'h28;
  localparam bit [7:0] OP_COBR_BBC       = 8'h30;
  localparam bit [7:0] OP_COBR_BBS       = 8'h37;
  localparam bit [7:0] OP_COBR_CMPIBE    = 8'h3A;
  localparam bit [7:0] OP_COBR_CMPIBNE   = 8'h3B;
  localparam bit [7:0] OP_COBR_CMPIBL    = 8'h3C;
  localparam bit [7:0] OP_COBR_CMPIBLE   = 8'h3D;
  localparam bit [7:0] OP_REG_ARITH_58   = 8'h58;
  localparam bit [7:0] OP_REG_ARITH_59   = 8'h59;
  localparam bit [7:0] OP_REG_ARITH_5A   = 8'h5A;
  localparam bit [7:0] OP_REG_LOGIC_5B   = 8'h5B;
  localparam bit [7:0] OP_REG_SHIFT_5C   = 8'h5C;
  localparam bit [7:0] OP_REG_BIT_5D     = 8'h5D;
  localparam bit [7:0] OP_REG_MOVE_5E    = 8'h5E;
  localparam bit [7:0] OP_REG_CTRL_60    = 8'h60;
  localparam bit [7:0] OP_REG_MULDIV_70  = 8'h70;
  localparam bit [7:0] OP_REG_MULDIV_74  = 8'h74;

  localparam bit [6:0] OP_EXT_NOTBIT     = 7'h00;
  localparam bit [6:0] OP_EXT_AND        = 7'h01;
  localparam bit [6:0] OP_EXT_ANDNOT     = 7'h02;
  localparam bit [6:0] OP_EXT_SETBIT     = 7'h03;
  localparam bit [6:0] OP_EXT_NOTAND     = 7'h04;
  localparam bit [6:0] OP_EXT_XOR        = 7'h06;
  localparam bit [6:0] OP_EXT_OR         = 7'h07;
  localparam bit [6:0] OP_EXT_NOR        = 7'h08;
  localparam bit [6:0] OP_EXT_XNOR       = 7'h09;
  localparam bit [6:0] OP_EXT_NOT        = 7'h0A;
  localparam bit [6:0] OP_EXT_ORNOT      = 7'h0B;
  localparam bit [6:0] OP_EXT_CLRBIT     = 7'h0C;
  localparam bit [6:0] OP_EXT_NOTOR      = 7'h0D;
  localparam bit [6:0] OP_EXT_NAND       = 7'h0E;
  localparam bit [6:0] OP_EXT_ALTERBIT   = 7'h0F;

  localparam bit [6:0] OP_EXT_ADDO       = 7'h20;
  localparam bit [6:0] OP_EXT_ADDI       = 7'h21;
  localparam bit [6:0] OP_EXT_SUBO       = 7'h22;
  localparam bit [6:0] OP_EXT_SUBI       = 7'h23;
  localparam bit [6:0] OP_EXT_CMPDO      = 7'h24;
  localparam bit [6:0] OP_EXT_CMPDI      = 7'h25;
  localparam bit [6:0] OP_EXT_CMPIO      = 7'h26;
  localparam bit [6:0] OP_EXT_CMPII      = 7'h27;
  localparam bit [6:0] OP_EXT_CMPO       = 7'h28;
  localparam bit [6:0] OP_EXT_CMPI       = 7'h29;
  localparam bit [6:0] OP_EXT_CONCMPO    = 7'h2A;
  localparam bit [6:0] OP_EXT_CONCMPI    = 7'h2B;
  localparam bit [6:0] OP_EXT_CHKBIT     = 7'h2C;

  localparam bit [6:0] OP_EXT_SHLO       = 7'h30;
  localparam bit [6:0] OP_EXT_SHRO       = 7'h31;
  localparam bit [6:0] OP_EXT_SHLI       = 7'h32;
  localparam bit [6:0] OP_EXT_SHRI       = 7'h33;
  localparam bit [6:0] OP_EXT_ROTATE     = 7'h34;
  localparam bit [6:0] OP_EXT_EXTRACT    = 7'h38;
  localparam bit [6:0] OP_EXT_MODIFY     = 7'h39;
  localparam bit [6:0] OP_EXT_SCANBIT    = 7'h3A;
  localparam bit [6:0] OP_EXT_SPANBIT    = 7'h3B;

  localparam bit [6:0] OP_EXT_MULO       = 7'h40;
  localparam bit [6:0] OP_EXT_MULI       = 7'h41;
  localparam bit [6:0] OP_EXT_DIVO       = 7'h42;
  localparam bit [6:0] OP_EXT_DIVI       = 7'h43;
  localparam bit [6:0] OP_EXT_REMO       = 7'h44;
  localparam bit [6:0] OP_EXT_REMI       = 7'h45;
  localparam bit [6:0] OP_EXT_EMUL       = 7'h46;
  localparam bit [6:0] OP_EXT_EDIV       = 7'h47;

  localparam bit [6:0] OP_EXT_MOV        = 7'h50;
  localparam bit [6:0] OP_EXT_MOVL       = 7'h51;
  localparam bit [6:0] OP_EXT_MOVT       = 7'h52;
  localparam bit [6:0] OP_EXT_MOVQ       = 7'h53;

  typedef enum logic [5:0] {
    ALU_NOP, ALU_ADD, ALU_SUB,
    ALU_AND, ALU_OR, ALU_XOR, ALU_NOT, ALU_NOR, ALU_NAND, ALU_XNOR,
    ALU_ANDNOT, ALU_NOTAND, ALU_ORNOT, ALU_NOTOR,
    ALU_SHL, ALU_SHR_LOGICAL, ALU_SHR_ARITH, ALU_ROTATE,
    ALU_SETBIT, ALU_CLRBIT, ALU_NOTBIT, ALU_ALTERBIT, ALU_CHKBIT,
    ALU_EXTRACT, ALU_MODIFY, ALU_SCANBIT, ALU_SPANBIT,
    ALU_CMPO, ALU_CMPI, ALU_PASSA, ALU_PASSB,
    ALU_MULO, ALU_MULI, ALU_DIVO, ALU_DIVI, ALU_REMO, ALU_REMI, ALU_EMUL, ALU_EDIV
  } alu_op_e;

  typedef struct packed {
    logic [10:0] reserved1;
    logic        no_imprecise_flt;
    logic [1:0]  rounding_mode;
    logic [9:0]  fp_masks_flags;
    logic        int_overflow_flg;
    logic        int_overflow_msk;
    logic [2:0]  reserved0;
    logic        cc_g; // CC2: Greater
    logic        cc_e; // CC1: Equal
    logic        cc_l; // CC0: Less
  } ac_reg_t;

  typedef struct packed {
    logic [11:0] reserved1;
    logic [4:0]  intr_priority;
    logic        reserved0;
    logic        intr_mask;
    logic [2:0]  reserved_bits;
    logic        exec_mode;
    logic        trace_enable;
    logic [7:0]  trace_events;
  } pc_reg_t;

  typedef struct packed {
    logic [31:0] pc;
    logic [31:0] next_pc;
    instr_fmt_e  fmt;
    logic [7:0]  opcode;
    logic [6:0]  opcode_ext;
    logic [4:0]  dst_reg;
    logic [4:0]  src1_reg;
    logic [4:0]  src2_reg;
    logic [4:0]  src3_reg;
    logic [31:0] src1_val;
    logic [31:0] src2_val;
    logic [31:0] src3_val;
    logic [31:0] imm_val;
    alu_op_e     alu_op;
    logic        is_branch;
    logic        is_call;
    logic        is_ret;
    logic        is_bal;
    logic        is_load;
    logic        is_store;
    logic        is_multi_word;
    logic [1:0]  multi_word_count; // 0=1w, 1=2w, 2=3w, 3=4w
    logic [1:0]  mem_size;         // 0=byte, 1=short, 2=word, 3=multi
    logic        mem_sign_extend;
    logic        reg_write_en;
    logic        update_ac_cc;
    logic        update_ac_flags;
    logic        is_fault;
    logic        is_sys_ctrl;
    logic [31:0] branch_target;
  } decoded_instr_t;

  typedef enum logic [1:0] {
    BUS_SIZE_BYTE  = 2'b00,
    BUS_SIZE_SHORT = 2'b01,
    BUS_SIZE_WORD  = 2'b10,
    BUS_SIZE_BURST = 2'b11
  } bus_size_e;

  typedef struct packed {
    logic        valid;
    logic        write_en;
    logic [31:0] addr;
    logic [31:0] wdata;
    logic [3:0]  byte_en;
    bus_size_e   size;
    logic [1:0]  burst_len;
  } bus_req_t;

  typedef struct packed {
    logic        ready;
    logic [31:0] rdata;
    logic        error;
  } bus_resp_t;

endpackage : open960_pkg

`endif // OPEN960_PKG_SV

  localparam bit [6:0] OP_EXT_CALLS      = 7'h60;
  localparam bit [6:0] OP_EXT_MODPC      = 7'h61;
  localparam bit [6:0] OP_EXT_MODAC      = 7'h62;
  localparam bit [6:0] OP_EXT_FLUSHREG   = 7'h63;

  localparam bit [7:0] OP_COBR_CMPIBG    = 8'h3E;
  localparam bit [7:0] OP_COBR_CMPIBGE   = 8'h3F;

  localparam bit [7:0] OP_MEM_LDOB       = 8'h80;
  localparam bit [7:0] OP_MEM_STOB       = 8'h82;
  localparam bit [7:0] OP_MEM_LDOS       = 8'h88;
  localparam bit [7:0] OP_MEM_STOS       = 8'h8A;
  localparam bit [7:0] OP_MEM_LDA        = 8'h8C;
  localparam bit [7:0] OP_MEM_LD         = 8'h90;
  localparam bit [7:0] OP_MEM_ST         = 8'h92;
  localparam bit [7:0] OP_MEM_LDL        = 8'h98;
  localparam bit [7:0] OP_MEM_STL        = 8'h9A;
  localparam bit [7:0] OP_MEM_LDT        = 8'hA0;
  localparam bit [7:0] OP_MEM_STT        = 8'hA2;
  localparam bit [7:0] OP_MEM_LDQ        = 8'hB0;
  localparam bit [7:0] OP_MEM_STQ        = 8'hB2;
  localparam bit [7:0] OP_MEM_LDIB       = 8'hC0;
  localparam bit [7:0] OP_MEM_STIB       = 8'hC2;
  localparam bit [7:0] OP_MEM_LDIS       = 8'hC8;
  localparam bit [7:0] OP_MEM_STIS       = 8'hCA;
