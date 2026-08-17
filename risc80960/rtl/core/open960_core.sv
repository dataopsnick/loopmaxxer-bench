// =============================================================================
// Open960 (RISC80960) - 32-bit Clean-Room Intel i960 Processor Core
// Top-Level Pipeline Integration
// =============================================================================

`ifndef OPEN960_CORE_SV
`define OPEN960_CORE_SV

`include "open960_pkg.sv"
`include "open960_control_regs.sv"
`include "open960_regfile.sv"
`include "open960_frame_cache.sv"
`include "open960_alu.sv"
`include "open960_muldiv.sv"
`include "open960_decoder.sv"
`include "open960_lsu.sv"
`include "open960_if_stage.sv"
`include "open960_hazard_unit.sv"

module open960_core
  import open960_pkg::*;
(
  input  logic        clk,
  input  logic        rst_n,

  // External Unified Memory / Bus Master Interface
  output bus_req_t    m_bus_req,
  input  bus_resp_t   m_bus_resp,

  // Hardware Interrupt Lines
  input  logic [3:0]  ext_intr_n,
  input  logic        ext_nmi_n
);

  // Pipeline Interconnect Signals
  logic        stall_if, stall_id, stall_ex;
  logic        flush_id, flush_ex;
  logic        branch_taken;
  logic [31:0] branch_target;

  // IF Stage Signals
  logic [31:0] if_pc, if_instr_w0, if_instr_w1;
  logic        if_valid, if_busy;
  bus_req_t    ibus_req;
  bus_resp_t   ibus_resp;

  // ID Stage Signals
  decoded_instr_t id_dec;
  logic        is_64bit;
  logic [31:0] rf_rdata1, rf_rdata2, rf_rdata3;
  logic [31:0] id_op1, id_op2, id_op3;
  logic [1:0]  fwd_src1_sel, fwd_src2_sel, fwd_src3_sel;

  // EX Stage Registers & Signals
  decoded_instr_t ex_dec;
  logic        ex_valid;
  logic [31:0] ex_op1, ex_op2, ex_op3;
  logic [31:0] alu_result;
  logic [2:0]  alu_cc_out;
  logic        alu_overflow;
  logic        muldiv_start, muldiv_busy, muldiv_done;
  logic [31:0] muldiv_res_low, muldiv_res_high;
  logic [31:0] ex_final_res;

  // Control Registers & Branch Resolution
  logic [31:0] ac_reg, pc_reg;
  logic        cond_branch_taken;
  logic [2:0]  branch_cond_type;

  // LSU Signals
  logic        lsu_busy, lsu_done;
  logic [31:0] lsu_rdata [0:3];
  logic [31:0] lsu_eff_addr;
  bus_req_t    dbus_req;
  bus_resp_t   dbus_resp;

  // Frame Cache Signals
  logic        fc_busy, fc_init_en;
  logic [31:0] fc_new_pfp, fc_new_sp, fc_new_rip, fc_new_fp;
  logic        fc_rf_we;
  logic [3:0]  fc_rf_rreg, fc_rf_sreg;
  logic [31:0] fc_rf_rdata, fc_rf_sdata;
  // ---------------------------------------------------------------------------
  // Submodule Instantiations
  // ---------------------------------------------------------------------------

  open960_if_stage u_if (
    .clk           (clk),
    .rst_n         (rst_n),
    .stall         (stall_if),
    .flush         (flush_id),
    .branch_taken  (branch_taken),
    .branch_target (branch_target),
    .if_pc         (if_pc),
    .if_instr_w0   (if_instr_w0),
    .if_instr_w1   (if_instr_w1),
    .if_valid      (if_valid),
    .if_busy       (if_busy),
    .ibus_req      (ibus_req),
    .ibus_resp     (ibus_resp)
  );

  open960_decoder u_dec (
    .pc            (if_pc),
    .instr_w0      (if_instr_w0),
    .instr_w1      (if_instr_w1),
    .dec           (id_dec),
    .is_64bit_instr(is_64bit)
  );

  open960_regfile u_rf (
    .clk               (clk),
    .rst_n             (rst_n),
    .raddr1            (id_dec.src1_reg),
    .rdata1            (rf_rdata1),
    .raddr2            (id_dec.src2_reg),
    .rdata2            (rf_rdata2),
    .raddr3            (id_dec.src3_reg),
    .rdata3            (rf_rdata3),
    .we                (wb_valid && wb_dec.reg_write_en),
    .waddr             (wb_dec.dst_reg),
    .wdata             (wb_res_w0),
    .we_word1          (wb_valid && wb_dec.is_multi_word && (wb_dec.multi_word_count >= 2'd1)),
    .waddr_word1       (wb_dec.dst_reg + 5'd1),
    .wdata_word1       (wb_res_w1),
    .we_word2          (wb_valid && wb_dec.is_multi_word && (wb_dec.multi_word_count >= 2'd2)),
    .waddr_word2       (wb_dec.dst_reg + 5'd2),
    .wdata_word2       (wb_res_w2),
    .we_word3          (wb_valid && wb_dec.is_multi_word && (wb_dec.multi_word_count >= 2'd3)),
    .waddr_word3       (wb_dec.dst_reg + 5'd3),
    .wdata_word3       (wb_res_w3),
    .frame_restore_we  (fc_rf_we),
    .frame_restore_reg (fc_rf_rreg),
    .frame_restore_data(fc_rf_rdata),
    .frame_spill_reg   (fc_rf_sreg),
    .frame_spill_data  (fc_rf_sdata)
  );

  open960_frame_cache u_fc (
    .clk              (clk),
    .rst_n            (rst_n),
    .cmd_call         (ex_valid && ex_dec.is_call),
    .cmd_ret          (ex_valid && ex_dec.is_ret),
    .cmd_flush        (ex_valid && (ex_dec.opcode_ext == OP_EXT_FLUSHREG)),
    .curr_pfp         (rf_rdata1),
    .curr_sp          (rf_rdata2),
    .curr_rip         (ex_dec.next_pc),
    .curr_fp          (rf_rdata3),
    .fsm_busy         (fc_busy),
    .new_pfp          (fc_new_pfp),
    .new_sp           (fc_new_sp),
    .new_rip          (fc_new_rip),
    .new_fp           (fc_new_fp),
    .new_frame_init_en(fc_init_en),
    .rf_restore_we    (fc_rf_we),
    .rf_restore_reg   (fc_rf_rreg),
  open960_alu u_alu (
    .alu_op      (ex_dec.alu_op),
    .op_a        (ex_op1),
    .op_b        (ex_op2),
    .op_c        (ex_op3),
    .ac_cc_in    (ac_reg[2:0]),
    .alu_result  (alu_result),
    .ac_cc_out   (alu_cc_out),
    .int_overflow(alu_overflow)
  );

  open960_muldiv u_muldiv (
    .clk        (clk),
    .rst_n      (rst_n),
    .start      (muldiv_start),
    .op         (ex_dec.alu_op),
    .op_a       (ex_op1),
    .op_b       (ex_op2),
    .op_b_high  (ex_op3),
    .busy       (muldiv_busy),
    .done       (muldiv_done),
    .res_low    (muldiv_res_low),
    .res_high   (muldiv_res_high)
  );

  open960_control_regs u_ctrl_regs (
    .clk               (clk),
    .rst_n             (rst_n),
    .ac_cc_update_en   (ex_valid && ex_dec.update_ac_cc),
    .ac_cc_in          (alu_cc_out),
    .ac_flags_update_en(ex_valid && ex_dec.update_ac_flags),
    .int_overflow_set  (alu_overflow),
    .modac_en          (ex_valid && (ex_dec.opcode_ext == OP_EXT_MODAC)),
    .modac_mask        (ex_op1),
    .modac_data        (ex_op2),
    .ac_reg_out        (ac_reg),
    .modpc_en          (ex_valid && (ex_dec.opcode_ext == OP_EXT_MODPC)),
    .modpc_mask        (ex_op1),
    .modpc_data        (ex_op2),
    .pc_reg_out        (pc_reg),
    .branch_cond_type  (branch_cond_type),
    .branch_taken      (cond_branch_taken)
  );

  open960_lsu u_lsu (
    .clk             (clk),
    .rst_n           (rst_n),
    .req_valid       (ex_valid && (ex_dec.is_load || ex_dec.is_store)),
    .is_load         (ex_dec.is_load),
    .is_store        (ex_dec.is_store),
    .is_multi_word   (ex_dec.is_multi_word),
    .multi_word_count(ex_dec.multi_word_count),
    .mem_size        (ex_dec.mem_size),
    .mem_sign_extend (ex_dec.mem_sign_extend),
    .base_val        (ex_op1),
    .index_val       (ex_op3),
    .scale_factor    (3'd0),
    .disp_val        (ex_dec.imm_val),
    .wdata_word0     (ex_op2),
    .wdata_word1     (32'h0),
    .wdata_word2     (32'h0),
    .wdata_word3     (32'h0),
    .lsu_busy        (lsu_busy),
    .lsu_done        (lsu_done),
    .rdata_word0     (lsu_rdata[0]),
    .rdata_word1     (lsu_rdata[1]),
    .rdata_word2     (lsu_rdata[2]),
    .rdata_word3     (lsu_rdata[3]),
    .effective_addr  (lsu_eff_addr),
    .bus_req         (dbus_req),
    .bus_resp        (dbus_resp)
  );

  open960_hazard_unit u_hazard (
    .id_src1_reg     (id_dec.src1_reg),
    .id_src2_reg     (id_dec.src2_reg),
    .id_src3_reg     (id_dec.src3_reg),
    .id_uses_src1    (1'b1),
    .id_uses_src2    (1'b1),
    .id_uses_src3    (id_dec.fmt == FMT_MEMB),
    .ex_dst_reg      (ex_dec.dst_reg),
    .ex_reg_write_en (ex_dec.reg_write_en),
    .ex_is_load      (ex_dec.is_load),
    .ex_is_multi_word(ex_dec.is_multi_word),
    .ex_multi_count  (ex_dec.multi_word_count),
    .mem_dst_reg     (wb_dec.dst_reg),
    .mem_reg_write_en(wb_dec.reg_write_en),
    .mem_is_multi_word(wb_dec.is_multi_word),
    .mem_multi_count (wb_dec.multi_word_count),
    .wb_dst_reg      (wb_dec.dst_reg),
  // ---------------------------------------------------------------------------
  // Pipeline Forwarding Multiplexers & Operands
  // ---------------------------------------------------------------------------
  always_comb begin
    case (fwd_src1_sel)
      2'b01:   id_op1 = ex_final_res;
      2'b10:   id_op1 = wb_res_w0;
      2'b11:   id_op1 = wb_res_w0;
      default: id_op1 = id_dec.imm_val != 32'h0 && (id_dec.fmt == FMT_COBR || id_dec.fmt == FMT_REG) ? id_dec.imm_val : rf_rdata1;
    endcase

    case (fwd_src2_sel)
      2'b01:   id_op2 = ex_final_res;
      2'b10:   id_op2 = wb_res_w0;
      2'b11:   id_op2 = wb_res_w0;
      default: id_op2 = rf_rdata2;
    endcase

    case (fwd_src3_sel)
      2'b01:   id_op3 = ex_final_res;
      2'b10:   id_op3 = wb_res_w0;
      2'b11:   id_op3 = wb_res_w0;
      default: id_op3 = rf_rdata3;
    endcase
  end

  // Branch Decision Logic
  always_comb begin
    branch_cond_type = id_dec.opcode[2:0];
    if (id_dec.fmt == FMT_CTRL) begin
      if (id_dec.opcode == OP_CTRL_B || id_dec.opcode == OP_CTRL_CALL || id_dec.opcode == OP_CTRL_BAL) begin
        branch_taken = if_valid;
      end else if (id_dec.opcode == OP_CTRL_RET) begin
        branch_taken = if_valid;
      end else begin
        branch_taken = if_valid && cond_branch_taken;
      end
    end else if (id_dec.fmt == FMT_COBR) begin
      branch_taken = if_valid && cond_branch_taken;
    end else begin
      branch_taken = 1'b0;
    end

    branch_target = (id_dec.opcode == OP_CTRL_RET) ? rf_rdata3 : id_dec.branch_target;
  end

  // MULDIV start trigger
  assign muldiv_start = ex_valid && (ex_dec.alu_op >= ALU_MULO && ex_dec.alu_op <= ALU_EDIV);
  assign ex_final_res = (ex_dec.alu_op >= ALU_MULO && ex_dec.alu_op <= ALU_EDIV) ? muldiv_res_low : 
                        ex_dec.is_load ? lsu_rdata[0] : 
                        alu_result;

  // ---------------------------------------------------------------------------
  // Bus Arbitrator (LSU Data > Frame Cache > Instruction Fetch)
  // ---------------------------------------------------------------------------
  always_comb begin
    m_bus_req    = ibus_req;
    ibus_resp    = m_bus_resp;
    dbus_resp    = '{ready: 1'b0, rdata: 32'h0, error: 1'b0};
    fc_bus_resp  = '{ready: 1'b0, rdata: 32'h0, error: 1'b0};

    if (dbus_req.valid) begin
      m_bus_req  = dbus_req;
      dbus_resp  = m_bus_resp;
      ibus_resp  = '{ready: 1'b0, rdata: 32'h0, error: 1'b0};
    end else if (fc_bus_req.valid) begin
      m_bus_req   = fc_bus_req;
      fc_bus_resp = m_bus_resp;
      ibus_resp   = '{ready: 1'b0, rdata: 32'h0, error: 1'b0};
    end
  end

  // ---------------------------------------------------------------------------
  // Pipeline Registers (ID -> EX -> WB)
  // ---------------------------------------------------------------------------
  always_ff @(posedge clk or negedge rst_n) begin
    if (!rst_n) begin
      ex_valid <= 1'b0;
      wb_valid <= 1'b0;
      ex_dec   <= '0;
      wb_dec   <= '0;
      ex_op1   <= 32'h0;
      ex_op2   <= 32'h0;
      ex_op3   <= 32'h0;
      wb_res_w0 <= 32'h0;
      wb_res_w1 <= 32'h0;
      wb_res_w2 <= 32'h0;
      wb_res_w3 <= 32'h0;
    end else begin
      // ID -> EX
      if (!stall_ex) begin
        if (flush_ex || !if_valid) begin
          ex_valid <= 1'b0;
          ex_dec   <= '0;
        end else begin
          ex_valid <= 1'b1;
          ex_dec   <= id_dec;
          ex_op1   <= id_op1;
          ex_op2   <= id_op2;
          ex_op3   <= id_op3;
        end
      end

      // EX -> WB
      if (!stall_ex) begin
        wb_valid  <= ex_valid;
        wb_dec    <= ex_dec;
        wb_res_w0 <= ex_final_res;
        wb_res_w1 <= (ex_dec.alu_op == ALU_EMUL || ex_dec.alu_op == ALU_EDIV) ? muldiv_res_high : lsu_rdata[1];
        wb_res_w2 <= lsu_rdata[2];
        wb_res_w3 <= lsu_rdata[3];
      end
    end
  end

endmodule : open960_core

`endif // OPEN960_CORE_SV

    .wb_reg_write_en (wb_dec.reg_write_en),
    .muldiv_busy     (muldiv_busy),
    .lsu_busy        (lsu_busy),
    .frame_cache_busy(fc_busy),
    .branch_taken    (branch_taken),
    .fwd_src1_sel    (fwd_src1_sel),
    .fwd_src2_sel    (fwd_src2_sel),
    .fwd_src3_sel    (fwd_src3_sel),
    .stall_if        (stall_if),
    .stall_id        (stall_id),
    .stall_ex        (stall_ex),
    .flush_id        (flush_id),
    .flush_ex        (flush_ex)
  );

    .rf_restore_data  (fc_rf_rdata),
    .rf_spill_reg     (fc_rf_sreg),
    .rf_spill_data    (fc_rf_sdata),
    .mem_req_valid    (fc_bus_req.valid),
    .mem_req_write    (fc_bus_req.write_en),
    .mem_req_addr     (fc_bus_req.addr),
    .mem_req_wdata    (fc_bus_req.wdata),
    .mem_resp_ready   (fc_bus_resp.ready),
    .mem_resp_rdata   (fc_bus_resp.rdata)
  );

  bus_req_t    fc_bus_req;
  bus_resp_t   fc_bus_resp;

  // WB Stage Signals
  decoded_instr_t wb_dec;
  logic        wb_valid;
  logic [31:0] wb_res_w0, wb_res_w1, wb_res_w2, wb_res_w3;
