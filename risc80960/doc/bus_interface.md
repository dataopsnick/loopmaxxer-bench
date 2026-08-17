# Open960 Bus Interface & Pin Compatibility

## 1. Intel i960 L-Bus Protocol
The Open960 processor integrates a cycle-accurate L-Bus interface unit (`open960_lbus_biu.sv`) compatible with original Intel 80960KA/KB/MC bus timings:

| Signal Name | Direction | Description |
|---|---|---|
| `CLK2` | Input | 2x System Clock frequency |
| `RESET#` | Input | System Reset (Active Low) |
| `LAD[31:0]` | Bidirectional | Multiplexed Address and Data Bus |
| `BE[3:0]#` | Output | Byte Enable signals for 8/16/32-bit transfers |
| `ADS#` | Output | Address Strobe (asserted on T1 phase) |
| `READY#` | Input | Target Device Ready acknowledge |
| `BLAST#` | Output | Burst Last cycle indicator |
| `W/R#` | Output | High = Write, Low = Read |
| `DEN#` | Output | Data Enable transceiver gate |
| `DT/R#` | Output | Data Direction control for external buffers |

## 2. Modern SoC Interfaces
For FPGA integration (LiteX, Vivado, Quartus) and ASIC SoC development, Open960 provides:
- **Wishbone B4 Master**: `rtl/bus/open960_wishbone_biu.sv`
- **AXI4-Lite Master**: `rtl/bus/open960_axi_biu.sv`
- **132-Pin PGA Physical Drop-in Wrapper**: `rtl/top/open960_top_pga132.sv`
