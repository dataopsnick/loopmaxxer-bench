# Agentic Self-Update Configuration (AGENTS.md)

This document coordinates the self-update loop and establishes safety guards to prevent destructive build loops.

## 1. Trigger
- **Manual:** Initiated via the `/benchmark escher` slash command.
- **Continuous:** Scheduled check, webhook automated test evaluations, or event-driven hook triggered when task resolutions are committed.

## 2. Safe Self-Update Execution Sequence
Rather than blindly copying and pushing files to the production branch, the agent must follow this local testing runbook inside the workspace container:

1. **Local Staging:** The agent pulls changes from `loopmaxxer-bench/escher/` into a local staging directory.
2. **Static Compilation Check:** Run compilation checks on the staged files to ensure no syntax regressions:
   ```bash
   python3 -m py_compile proxy.py generate_tasklist.py ci_reviewer.py
   ```
3. **Deterministic Unit Testing:** Execute the entire verification suite locally:
   ```bash
   python3 -m unittest discover -s tests
   ```
4. **Uptime Preflight Check:** Dry-run the proxy process locally using a short-lived test instance to ensure FastAPI routes load without throwing runtime module imports or startup exceptions.
5. **open-code-review scan** Run `./ocr scan --path loopmaxxer-bench/escher/` to capture code state.
6. **Generate TASKLIST.md** Parse and structure findings using `generate_tasklist.py`.
7. **Strange Loop** Apply the LLM autonomous editing loop to target files under `loopmaxxer-bench/escher/`.
8. **Write Unit Test** Dynamically write a basic unit test file in `tests/test_task_<ID>.py` inside the cloned repository to pass Gatekeeper validation checks.
9. **Git Push:** Only if steps 2, 3, 4, 8 return `exit code 0` is the agent permitted to execute the commit and push to the production repository via `/action/create-pr`.

## 3. Stop Rules & Circuit Breakers
- **Single-Failure Halt:** If any unit test, syntax compilation, or static check fails during staging, the self-update is aborted immediately. The workspace reverts to the last known stable commit.
- **Uptime Guard:** The agent is strictly forbidden from editing the network ingress parameters or modifying ports (`7860`, `8080`) unless explicitly directed by a signed goal configuration.
- **Budget Bound:** Limit loop operations to a maximum of 3 consecutive self-updates per 24-hour window to protect compute tier resources.
