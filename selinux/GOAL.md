# Goal: Lightweight Static Analysis Loop

Target and inspect specific implementation files in target repositories for coding standard compliance, memory safety, or specific check patterns without full-cloning.

## Core Objective
- Dynamically query repository trees.
- Lazy-load and analyze target `.c` and `.cpp` files individually.
- Summarize analysis results into `CODEREVIEW.md` and track progress in `progress.md`.