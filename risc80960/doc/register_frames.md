# Open960 Procedure Call & Register Window Frame Cache

## 1. Local Register Window Concept
Rather than forcing software compilers to generate explicit memory pushes/pops on every procedure call, the i960 architecture maintains local registers `r0` - `r15` as dynamic hardware stack frames.

## 2. Procedure Call Sequence (`call`, `callx`, `calls`)
When a procedure call is executed:
1. The hardware saves the caller's active local registers (`r0-r15`) into the on-chip circular frame buffer (default capacity: 4 to 8 procedure frames).
2. The caller's `FP` (`g15`) is saved into the new frame's `PFP` (`r0`).
3. The return address (`PC + 4` or `PC + 8`) is placed into `RIP` (`r2`).
4. The stack pointer `SP` (`r1`) is allocated at the next 64-byte boundary: `(FP + 63) & ~63`.
5. The frame pointer `FP` is updated to point to the current frame boundary.

## 3. Automatic Spill / Fill Engine
- **Spill on Call**: When nested procedure calls exceed the physical on-chip frame cache depth, the hardware Spill state machine automatically bursts the oldest cached frame out to system memory at its corresponding `PFP` address.
- **Fill on Return**: When nested procedure returns underflow the on-chip frame cache, the hardware Fill state machine reloads the 16 local registers from memory at `PFP`.
- **Zero Overhead**: In standard call depth workloads (e.g. depth <= 4), procedure calls and returns execute in zero memory cycles.
