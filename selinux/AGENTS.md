# Agentic Loop Configuration (AGENTS.md)

This file defines the lazy-fetching review execution loop.

## 1. Trigger
- **Definition:** Triggered via `/benchmark selinux` command or via manual execution loop.

## 2. Execution Runbook
1. Query GitHub Git Trees API for the specified repository.
2. Parse path list and isolate files ending with `.c` or `.cpp`.
3. Fetch the content of each file sequentially using the GitHub Contents API.
4. Pass the retrieved code content to the analyzer model to evaluate safety checks.
5. Record findings in `CODEREVIEW.md`.

## 3. Verifier
- Validate that the fetched files compile or parse cleanly via local AST verification.

## 4. Stop Rules
- Limit analysis to a maximum of 10 files per run, or stop upon encountering a critical API rate limit.