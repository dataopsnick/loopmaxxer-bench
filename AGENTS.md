# Agentic Loop Configuration (AGENTS.md)

This file defines the repository-wide agentic loop configuration and safety gates under the **Loop Engineering 101** framework. It coordinates how autonomous agents process tasks, run validations, and perform merges across individual benchmarks (e.g., `market-making`, `selinux`, etc.) [CODEREVIEW.md].

---

## 1. Trigger
* **Definition:** Triggered manually via the `/review-prs` or `/ci-prs` chat slash commands, or automatically when a new pull request with an agent-initiated head branch (matching the `ocr-tasks-` naming convention) is created [CODEREVIEW.md].

---

## 2. Execution Runbook (The Verification & CI Merge Cycle)

Before a pull request can be merged, the agent must check out the target branch and execute its benchmark-specific validation tests [CODEREVIEW.md]. This execution runs inside a secure, isolated sandbox environment to prevent untrusted execution side effects.

### Step-by-Step Execution Sequence

1. **Active PR Discovery:**
   * Query the GitHub Pull Requests API (`/repos/{owner}/{repo}/pulls?state=open`) and isolate open PRs originating from agent branches (`ocr-tasks-*`) [2.1].

2. **Secure Sandbox Provisioning:**
   * Invoke the **`gemini-interactions-api`** / **`gemini_interactions_api`** skill to spin up a managed remote Linux container (`environment: "remote"`).
   * Declaratively mount the repository and checkout the specific pull request branch.

3. **Benchmark Path Resolution:**
   * Analyze the modified files in the pull request to identify the target benchmark context (e.g., `market-making/`, `selinux/`, etc.) [CODEREVIEW.md].

4. **Sandboxed Verification Test Run:**
   * Inside the provisioned environment, navigate to the targeted benchmark subdirectory [CODEREVIEW.md].
   * Run the test discovery script to locate and execute unit tests within that benchmark's `tests/` directory [CODEREVIEW.md].
   * Execute the testing suite using:
     ```bash
     python3 -m unittest discover -s tests
     ```
   * Retrieve the complete step logs and exit status via the `interaction.steps` API history to verify whether all assertions passed with `exit code 0`.

5. **Code Review Generation (GLM 5.2):**
   * Execute the `ocr review` auditing cycle inside the sandbox comparing the base and head branch [1.2.3, GEMINI_SYSTEM_PROMPT.md.txt].
   * Generate the `CODEREVIEW.md` diagnostics report [CODEREVIEW.md].

6. **Pull Request Submission & Merge Gate:**
   * Forward the verified test results and the compiled code review output to `ci_reviewer.py` [CODEREVIEW.md].
   * `ci_reviewer.py` posts the results back to the GitHub PR as a comment and executes a secure Squash Merge if, and only if, all tests have passed [1.1.2, CODEREVIEW.md].

---

## 3. Verifier (The Merge Gatekeeper)
* **Validator:** `tests/` directories matching the active benchmark, `verify_xml.py` structure validation [CODEREVIEW.md].
* **Rule:** If any assertion fails inside the `gemini-interactions-api` execution snapshot or if a benchmark-specific test file is missing, the verifier fails the execution loop [GEMINI_SYSTEM_PROMPT.md.txt]. Merging via `ci_reviewer.py` is blocked [CODEREVIEW.md].

---

## 4. Stop Rules
* **Budget Limits:** Max 300,000 tokens per interaction loop [GEMINI_SYSTEM_PROMPT.md.txt].
* **Execution Limit:** Max 5 consecutive CLI scan operations [CODEREVIEW.md].
* **Circuit Breaker:** Halt operations immediately on any test failures, API rate limits (HTTP 429), or missing setup configurations [CODEREVIEW.md].

---

## 5. Memory
* **State Management:** Track active processes using `setup_state.json` and `TASKLIST.md` [CODEREVIEW.md].
* **Sandbox Tracking:** Persist and re-use the sandbox `environment_id` returned by the Interactions API across consecutive validation turns to preserve the state of checked-out repositories [GEMINI_SYSTEM_PROMPT.md.txt].

---

## 6. Configured Skills
* **`loop-engineer`**: Coordinates trigger states and the verification lifecycles.
* **`pr-fix-verifier`**: Standardizes the pattern of writing deterministic, non-destructive test assertions inside the `tests/` subfolder.
* **`gemini-interactions-api`**: Manages isolated remote Linux container instances to securely compile code and run tests without polluting the primary runtime environment [GEMINI_SYSTEM_PROMPT.md.txt].
