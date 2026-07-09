#!/usr/bin/env python3
import os
import re
import sys
import json
import logging
import asyncio
import httpx

# Configure Logging
logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)]
)

# Global Settings (overridable via Environment Variables)
GITHUB_TOKEN = os.getenv("GITHUB_TOKEN")
REPO_OWNER = os.getenv("REPO_OWNER", "dataopsnick")
REPO_NAME = os.getenv("REPO_NAME", "loopmaxxer-bench")
BASE_BRANCH = os.getenv("BASE_BRANCH", "main")
OCR_BIN = os.getenv("OCR_BIN", "./ocr")

async def run_cmd(args):
    """Asynchronously executes shell commands and returns output/exit code."""
    proc = await asyncio.create_subprocess_exec(
        *args,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE
    )
    stdout, stderr = await proc.communicate()
    return proc.returncode, stdout.decode('utf-8', errors='replace').strip(), stderr.decode('utf-8', errors='replace').strip()

async def get_open_pull_requests(client: httpx.AsyncClient):
    """Fetches all open pull requests from the repository."""
    url = f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/pulls?state=open"
    headers = {
        "Authorization": f"token {GITHUB_TOKEN}",
        "Accept": "application/vnd.github+json"
    }
    logging.info("Checking for open Pull Requests requesting merge...")
    r = await client.get(url, headers=headers)
    if r.status_code == 200:
        return r.json()
    else:
        logging.error(f"Failed to query PRs: {r.status_code} - {r.text}")
        return []

async def fetch_and_checkout_pr(pull_number, branch_name):
    """Pulls the remote PR branch and checks it out locally."""
    logging.info(f"Synchronizing PR #{pull_number} locally onto branch: {branch_name}")
    # 1. Fetch branch
    ret, _, err = await run_cmd(["git", "fetch", "origin", f"pull/{pull_number}/head:{branch_name}"])
    if ret != 0:
        logging.error(f"Fetch failed: {err}")
        return False
    # 2. Checkout branch
    ret, _, err = await run_cmd(["git", "checkout", branch_name])
    if ret != 0:
        logging.error(f"Checkout failed: {err}")
        return False
    return True

import os
import re
import logging

async def run_deterministic_gatekeeper_tests(branch_name: str) -> bool:
    """
    Acts as an absolute, deterministic software gate for merging.
    
    1. Parses the Task ID from the branch name (expected format: ocr-tasks-<ID>).
    2. Ensures the required test file (tests/test_task_<ID>.py) exists on disk.
    3. Performs a compilation check on all Python files to prevent syntax errors.
    4. Runs XML structure verification on TASKLIST.md.
    5. Executes the targeted unit test suite.
    
    Returns True ONLY if all checks strictly clear with exit code 0.
    """
    logging.info(f"Initiating Gatekeeper validation on branch: {branch_name}")
    
    # 1. Parse Task ID from Branch Name (e.g., 'ocr-tasks-1' -> ID 1)
    match = re.search(r"ocr-tasks-(\d+)", branch_name)
    if not match:
        logging.error(
            f"Gatekeeper Rejected: Branch name '{branch_name}' does not match "
            "the expected agent convention 'ocr-tasks-<ID>'."
        )
        return False
    
    task_id = match.group(1)
    required_test_file = f"tests/test_task_{task_id}.py"
    
    # 2. Enforce physical presence of the matching test file
    if not os.path.exists(required_test_file):
        logging.error(
            f"Gatekeeper Tripped: Missing required verification test. "
            f"Each bugfix branch must be accompanied by '{required_test_file}'."
        )
        return False
    
    # 3. Compilation check on all Python files to prevent broken syntax imports
    logging.info("Compiling codebase Python files...")
    syntax_passed = True
    for root, _, files in os.walk("."):
        if any(exclude in root for exclude in ("node_modules", ".git", "venv", "dist", "build")):
            continue
        for file in files:
            if file.endswith(".py"):
                py_file = os.path.join(root, file)
                # run_cmd is your async helper that executes subprocesses
                ret, _, err = await run_cmd(["python3", "-m", "py_compile", py_file])
                if ret != 0:
                    logging.error(f"Syntax compilation error in '{py_file}': {err}")
                    syntax_passed = False
                    
    if not syntax_passed:
        logging.error("Gatekeeper Tripped: Syntax compilation errors detected in workspace.")
        return False

    # 4. Validate TASKLIST.md syntax if verify_xml.py exists
    if os.path.exists("TASKLIST.md") and os.path.exists("verify_xml.py"):
        logging.info("Validating TASKLIST.md structure...")
        ret, out, _ = await run_cmd(["python3", "verify_xml.py"])
        if "SUCCESS" not in out:
            logging.error("Gatekeeper Tripped: verify_xml validation failed on TASKLIST.md")
            return False

    # 5. Programmatically execute the targeted unit test
    logging.info(f"Executing targeted verification suite: {required_test_file}")
    # Run unittest discovery specifically targeting the task file
    ret, stdout, stderr = await run_cmd([
        "python3", "-m", "unittest", "discover", 
        "-s", "tests", 
        "-p", f"test_task_{task_id}.py"
    ])
    
    if ret != 0:
        logging.error(f"Gatekeeper Tripped: Target unit test execution failed with exit code {ret}.")
        logging.error(f"Execution output (STDOUT):\n{stdout}")
        logging.error(f"Execution output (STDERR):\n{stderr}")
        return False
        
    logging.info(f"✅ Gatekeeper Cleared: '{required_test_file}' passed successfully.")
    return True

async def run_automated_tests(branch_name):
    """Runs local syntax checks and semantic XML checks on modified files."""
    logging.info("Running automated test actions...")
    
    # Check syntax validation on python files
    syntax_passed = True
    for root, _, files in os.walk("."):
        if "node_modules" in root or ".git" in root or "venv" in root:
            continue
        for file in files:
            if file.endswith(".py"):
                py_file = os.path.join(root, file)
                ret, _, err = await run_cmd(["python3", "-m", "py_compile", py_file])
                if ret != 0:
                    logging.warning(f"Syntax error found in modified file: {py_file} -> {err}")
                    syntax_passed = False
                    
    # Validate TASKLIST.md syntax if modified
    xml_passed = True
    if os.path.exists("TASKLIST.md") and os.path.exists("verify_xml.py"):
        ret, out, _ = await run_cmd(["python3", "verify_xml.py"])
        if "SUCCESS" not in out:
            logging.warning("verify_xml validation failed on TASKLIST.md")
            xml_passed = False
            
    return syntax_passed and xml_passed

async def run_alibaba_ocr_review(branch_name):
    """Invokes Open Code Review to audit changes using z-ai/glm-5.2."""
    logging.info(f"Invoking Alibaba Open Code Review (comparing {BASE_BRANCH} -> {branch_name})...")
    
    # This runs the compiled OCR CLI, automatically capturing reviews to CODEREVIEW.md
    # The proxy will intercept and process this using the designated primary LLM configuration
    ret, out, err = await run_cmd([OCR_BIN, "review", "--from", BASE_BRANCH, "--to", branch_name])
    if ret != 0:
        logging.warning(f"Alibaba OCR warning/failure during review: {err}")
        
    review_content = ""
    if os.path.exists("CODEREVIEW.md"):
        with open("CODEREVIEW.md", "r", encoding="utf-8") as f:
            review_content = f.read()
            
    return review_content

async def submit_github_review(client: httpx.AsyncClient, pull_number, review_text, passed):
    """Submits the compiled OCR review and testing state back to the GitHub PR."""
    logging.info(f"Submitting review feedback to PR #{pull_number}...")
    url = f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{pull_number}/reviews"
    headers = {
        "Authorization": f"token {GITHUB_TOKEN}",
        "Accept": "application/vnd.github+json"
    }
    
    status_msg = "✅ **AUTOMATED INTEGRATION RESULTS:**\n" if passed else "⚠️ **AUTOMATED INTEGRATION WARNINGS:**\n"
    status_msg += "All local syntax and validation checks have passed successfully!\n\n" if passed else "Errors were found during syntax or tasklist verification.\n\n"
    
    body = (
        f"{status_msg}"
        f"### Alibaba Open Code Review Auditing Feedback (GLM 5.2):\n"
        f"```markdown\n"
        f"{review_text if review_text else '(No issues found by OCR review)'}\n"
        f"```"
    )
    
    payload = {
        "body": body,
        "event": "COMMENT" if not passed else "APPROVE"
    }
    
    r = await client.post(url, json=payload, headers=headers)
    if r.status_code == 201:
        logging.info(f"Successfully posted review on PR #{pull_number}")
        return True
    else:
        logging.error(f"Failed to post PR review: {r.status_code} - {r.text}")
        return False

async def merge_pull_request(client: httpx.AsyncClient, pull_number):
    """Merges the Pull Request via Squash Merge once checks have cleared."""
    logging.info(f"Merging PR #{pull_number} into {BASE_BRANCH}...")
    url = f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{pull_number}/merge"
    headers = {
        "Authorization": f"token {GITHUB_TOKEN}",
        "Accept": "application/vnd.github+json"
    }
    payload = {
        "commit_title": f"chore: squash merge PR #{pull_number} following agent validation pass",
        "merge_method": "squash"
    }
    r = await client.put(url, json=payload, headers=headers)
    if r.status_code == 200:
        logging.info(f"PR #{pull_number} successfully merged!")
        return True
    else:
        logging.error(f"Failed to merge PR #{pull_number}: {r.status_code} - {r.text}")
        return False

async def main():
    if not GITHUB_TOKEN or GITHUB_TOKEN == "none":
        logging.error("Missing GITHUB_TOKEN environment variable. Terminating execution.")
        return

    async with httpx.AsyncClient(timeout=60.0) as client:
        # 1. Fetch active PRs
        prs = await get_open_pull_requests(client)
        if not prs:
            logging.info("No open Pull Requests found.")
            return
            
        # Target PRs generated by the loop agent (e.g. branch matching 'ocr-tasks-')
        agent_prs = [pr for pr in prs if pr["head"]["ref"].startswith("ocr-tasks-")]
        if not agent_prs:
            logging.info("No active agent-initiated merge requests identified.")
            return
            
        logging.info(f"Found {len(agent_prs)} active agent merge request(s). Starting CI cycle...")
        
        for pr in agent_prs:
            pull_number = pr["number"]
            head_branch = pr["head"]["ref"]
            local_branch = f"pr-{pull_number}"
            
            # 2. Local Checkout
            checkout_success = await fetch_and_checkout_pr(pull_number, local_branch)
            if not checkout_success:
                continue
                
            # 3. Compile and Run Automated Tests (enforcing the Gatekeeper rule)
            tests_passed = await run_deterministic_gatekeeper_tests(head_branch) # await run_automated_tests(local_branch)
            
            # 4. Invoke Open Code Review (routes to z-ai/glm-5.2)
            review_text = await run_alibaba_ocr_review(local_branch)
            
            # 5. Post findings
            review_submitted = await submit_github_review(client, pull_number, review_text, tests_passed)
            
            # 6. Merge if verified successfully
            if tests_passed and review_submitted:
                await merge_pull_request(client, pull_number)
                
            # 7. Return back to base branch and clean up
            await run_cmd(["git", "checkout", BASE_BRANCH])
            await run_cmd(["git", "branch", "-D", local_branch])

if __name__ == "__main__":
    asyncio.run(main())
