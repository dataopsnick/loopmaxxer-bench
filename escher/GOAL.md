# Goal: The Escher Recursive Self-Optimization Loop (GOAL.md)

This file defines the ultimate target state and performance metrics for the self-updating Escher loop.

## Core Objective
The objective is to iteratively eliminate all critical, high, and medium-severity code defects, security issues, and architectural vulnerabilities in the active workspace as scanned by the Alibaba Open Code Review (OCR) utility, while maintaining absolute system uptime and operational parity.

## The Compass (True North Metrics)
1. **Zero Runtime Regression (Absolute Gate):** The primary metric is system stability. No code modification is allowed to merge or deploy unless it passes 100% of the unit test validations and compilation checks.
2. **Deterministic Integrity:** Every applied fix must be accompanied by an isolated unit test in `tests/test_task_<ID>.py` that explicitly asserts the correct behavior of the modified component.
3. **Secret Sanitization:** Enforce strict containment of sensitive tokens (`HF_TOKEN`, `GITHUB_TOKEN`, `OpenRouter API Keys`). No cryptographic keys or bear tokens may ever be written to disk, process listings, command traces, or git history.
4. **Task Burn-Down Efficiency:** The loop is successful when the number of outstanding tasks in the generated XML `TASKLIST.md` monotonically decreases to zero.

## Reference Frameworks
- **Verification Protocol:** `skills/pr-fix-test-engineer/SKILL.md`
- **Loop Engineering Principles:** `skills/loop-engineer/SKILL.md`
