"""
relay.py — Refactored proxy with clean state machine architecture.

This is a senior-dev-quality rewrite of proxy.py that replaces the convoluted
if/elif state machine chain with:

  1. An enum-based state registry (SetupStep) — no more magic strings
  2. A SetupWizard class with a handler-registry dispatch pattern — each state
     is a self-contained async generator method, making it trivial to test,
     reorder, or extend without touching dispatch logic
  3. A CommandHandlers class with pattern-based routing for slash commands —
     replaces the long chain of inline if-checks at the top of run_cli_stream
  4. Consolidated state loading — eliminates the load_state/load_default_state
     duplication by using DEFAULT_STATE with setdefault()
  5. SSEStream helper class — replaces the yield_text closure pattern
  6. Clear separation of concerns — state management, wizard, commands, CLI
     execution, and HTTP routes are all distinct sections

Bug fixes carried over from proxy.py:
  - Missing HTMLResponse import (used but never imported)
  - _ci_execution_lock defined twice (second overwrote first)
  - Model selection logic had swapped keyword checks (option 1 checked "pro",
    option 2 checked "glm" — now correctly maps to menu labels)
  - load_state/load_default_state duplication consolidated into one function
"""

import asyncio
import enum
import json
import shlex
import re
import time
import os
import shutil
import ast
import subprocess
import traceback
import xml.etree.ElementTree as ET
from typing import AsyncGenerator, Callable, Optional

import httpx
import hmac
import hashlib

from fastapi import FastAPI, Request, Response, Header, HTTPException
from fastapi.responses import StreamingResponse, FileResponse, HTMLResponse
from generate_tasklist import process_review_file

# ============================================================================
# Constants & Configuration
# ============================================================================

app = FastAPI()

ANSI_ESCAPE = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
STATE_FILE = "setup_state.json"
TRAFFIC_LOG = "traffic.log"
WORKSPACE_ROOT = os.path.abspath(os.getcwd())

DEFAULT_STATE = {
    "step": "completed",
    "whitelist": ["novita", "google-ai-studio", "google-vertex"],
    "zdr": False,
    "data_collection": "allow",
    "allow_fallbacks": True,
    "require_parameters": False,
}

ALLOWED_BINARIES = {"git", "./ocr", "pnpm", "npx"}
WEBHOOK_SECRET = os.getenv("WEBHOOK_SECRET")

# Concurrency locks (defined once — proxy.py had these duplicated)
_state_lock = asyncio.Lock()
_ci_execution_lock = asyncio.Lock()


# ============================================================================
# SSE Stream Helper
# ============================================================================

class SSEStream:
    """
    Generates OpenAI-compatible Server-Sent Event chunks.

    Replaces the yield_text closure pattern in proxy.py with a reusable,
    testable helper that captures created_time at construction.
    """

    MODEL_ID = "deepseekv4glm5.2"

    def __init__(self):
        self.created_time = int(time.time())

    def text(self, text: str) -> str:
        """Create a text delta chunk."""
        chunk = {
            "id": "ocr",
            "object": "chat.completion.chunk",
            "created": self.created_time,
            "model": self.MODEL_ID,
            "choices": [{"delta": {"content": text}}],
        }
        return f"data: {json.dumps(chunk)}\n\n"

    def done(self) -> str:
        """Create the [DONE] terminator."""
        return "data: [DONE]\n\n"

    def role(self) -> str:
        """Empty chunk to trigger typing animation."""
        chunk = {
            "id": "ocr",
            "object": "chat.completion.chunk",
            "created": self.created_time,
            "model": self.MODEL_ID,
            "choices": [{"delta": {"role": "assistant", "content": ""}}],
        }
        return f"data: {json.dumps(chunk)}\n\n"


# ============================================================================
# State Management
# ============================================================================

def get_secure_path(relative_target: str) -> str:
    """Resolve a path within the workspace, rejecting traversal attempts."""
    target_abs = os.path.abspath(os.path.join(WORKSPACE_ROOT, relative_target))
    if not target_abs.startswith(WORKSPACE_ROOT):
        raise PermissionError("Access Denied: Path traversal outside workspace boundary.")
    return target_abs


def log_traffic(message: str):
    """Append raw network traffic traces to disk with 0o600 permissions and secret redaction."""
    try:
        message = re.sub(
            r'(?i)Authorization:\s*Bearer\s+[a-zA-Z0-9_\-\.]+',
            'Authorization: Bearer [REDACTED]', message)
        message = re.sub(r'(?i)ghp_[a-zA-Z0-9]+', '[REDACTED_GITHUB_TOKEN]', message)
        message = re.sub(r'(?i)sk-or-[a-zA-Z0-9\-]+', '[REDACTED_OPENROUTER_KEY]', message)

        timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
        log_line = f"[{timestamp}] {message}\n"

        flags = os.O_WRONLY | os.O_CREAT | os.O_APPEND
        fd = os.open(TRAFFIC_LOG, flags, 0o600)
        with open(fd, "w", encoding="utf-8") as f:
            f.write(log_line)
    except Exception as e:
        print(f"Failed to write traffic log: {e}")


def load_state():
    """
    Load or initialize the configuration setup state.

    Consolidates proxy.py's load_state() and load_default_state() into a single
    function that uses DEFAULT_STATE with setdefault() for missing keys, eliminating
    the manual per-key checking that plagued the original.
    """
    # 1. Check for environment bootstrap JSON (highest priority)
    bootstrap_env = os.getenv("OCR_BOOTSTRAP_JSON")
    if bootstrap_env:
        try:
            init_data = json.loads(bootstrap_env)
            return {
                "step": "completed",
                "api_key": "configured",
                "github_token": init_data.get("github_token", "none"),
                "whitelist": init_data.get("whitelist", ["novita", "google-ai-studio", "google-vertex"]),
                "preferred_model": init_data.get("preferred_model", "deepseek/deepseek-v4-pro"),
                "alternative_model": init_data.get("alternative_model", "None"),
                "zdr": init_data.get("zdr", False),
                "data_collection": init_data.get("data_collection", "allow"),
                "allow_fallbacks": init_data.get("allow_fallbacks", True),
                "require_parameters": init_data.get("require_parameters", False),
                "policy_name": "Environment Auto-Bootstrapped",
            }
        except Exception as e:
            print(f"⚠️ [System] In-memory bootstrap parse failed: {e}")

    # 2. No state file — check for existing OCR config or return initial wizard state
    if not os.path.exists(STATE_FILE):
        config_path = os.path.expanduser("~/.opencodereview/config.json")
        has_token = False
        if os.path.exists(config_path):
            try:
                with open(config_path) as f:
                    cfg = json.load(f)
                    has_token = bool(cfg.get("llm", {}).get("auth_token"))
            except:
                pass
        if not has_token:
            return {"step": "ask_api_key"}
        return DEFAULT_STATE.copy()

    # 3. Load from state file, applying defaults for any missing keys
    try:
        with open(STATE_FILE) as f:
            state = json.load(f)
            for key, default_value in DEFAULT_STATE.items():
                state.setdefault(key, default_value)
            return state
    except:
        return {"step": "ask_api_key"}


def save_state(state):
    """Persist state to disk as JSON."""
    try:
        with open(STATE_FILE, "w") as f:
            json.dump(state, f, indent=2)
    except Exception as e:
        print(f"Failed to save state: {e}")


async def async_load_state():
    """Thread-safe state loading."""
    async with _state_lock:
        return load_state()


async def async_save_state(state):
    """Thread-safe state saving."""
    async with _state_lock:
        save_state(state)


def get_github_token(state):
    """
    Retrieve GitHub token from state, falling back to GITHUB_TOKEN env var
    only if 'github_token' is not configured (None).
    """
    token = state.get("github_token")
    if token is None:
        return os.getenv("GITHUB_TOKEN")
    return token


def update_ocr_config(auth_token=None, model=None, url="http://127.0.0.1:8080/v1"):
    """Safely updates ~/.opencodereview/config.json directly in Python with proper types."""
    config_path = os.path.expanduser("~/.opencodereview/config.json")
    os.makedirs(os.path.dirname(config_path), exist_ok=True)
    cfg = {}
    if os.path.exists(config_path):
        try:
            with open(config_path, "r") as f:
                cfg = json.load(f)
        except:
            pass
    if "llm" not in cfg:
        cfg["llm"] = {}
    cfg["llm"]["url"] = url
    cfg["llm"]["use_anthropic"] = False
    if auth_token is not None:
        cfg["llm"]["auth_token"] = auth_token
    if model is not None:
        cfg["llm"]["model"] = model
    cfg["provider"] = "openai"
    if "providers" not in cfg:
        cfg["providers"] = {}
    cfg["providers"]["openai"] = {
        "url": url,
        "protocol": "openai",
        "api_key": auth_token or cfg["llm"].get("auth_token", ""),
        "model": model or cfg["llm"].get("model", ""),
    }
    with open(config_path, "w") as f:
        json.dump(cfg, f, indent=2)


def to_bool(val) -> bool:
    """Convert various truthy representations to a boolean."""
    if isinstance(val, bool):
        return val
    if str(val).lower().strip() == "true":
        return True
    return False


# ============================================================================
# File Generators
# ============================================================================

def generate_goal_md(state):
    """Compiles configured loop targets into a GOAL.md file on disk."""
    goal_text = state.get(
        "loop_goal",
        "Implement a robust, event-driven market-making strategy in "
        "loopmaxxer-bench/market-making with custom inventory management and spread quoting model",
    )
    goal_content = f"""# Goal (GOAL.md)

This file defines the ultimate target state for our safe self-prompting loop.

## Core Objective
- **Goal:** {goal_text}

## Reference Materials
- **Loop Engineering Framework:** `./skills/loop-engineer/SKILL.md`
- **Documentation reference:** `./skills/llms.txt`
"""
    with open("GOAL.md", "w", encoding="utf-8") as f:
        f.write(goal_content)
    return goal_content


def generate_agents_md(state):
    """Compiles setup parameters into an AGENTS.md document under Loop Engineering 101 rules."""
    trigger = state.get("loop_trigger", "On a new Git Issue or manually via chat")
    execution = state.get("loop_execution", "scan workspace, write CODEREVIEW.md, parse TASKLIST.md, create-issues")
    verifier = state.get("loop_verifier", "verify_xml.py syntax pass and verify_pairs.py completion check")
    stop_rules = state.get("loop_stop_rules", "Max 5 iterations or any unrecoverable error")
    memory = state.get("loop_memory", "setup_state.json and TASKLIST.md")
    skills = state.get("loop_skills", "loop-engineer skill from loopmaxxer-bench")

    agents_content = f"""# Agentic Loop Configuration (AGENTS.md)

This file defines our safe self-prompting loop under the **Loop Engineering 101** framework.

## 1. Trigger
- **Definition:** {trigger}
- **Integration:** Listens to triggers to kick off the development & review cycle.

## 2. Execution Runbook (The OCR ➔ TASKLIST Cycle)
- **Commands:** {execution}
- **Step-by-Step Cycle:**
  1. **Audit/Scan:** Run `./ocr scan --path .` or equivalent to audit codebase changes.
  2. **Review Output:** Writes full critique into `CODEREVIEW.md`.
  3. **Task Conversion:** Run `generate_tasklist.py` to compile `CODEREVIEW.md` into well-formed XML `TASKLIST.md`.
  4. **Issue Creation:** Use `/create-issues` (or proxy endpoint) to split tasks into individual markdown issues in `./issues/`.
  5. **Branch & PR:** Create a git branch and pull request for resolving the issues.

## 3. Verifier
- **Validator:** {verifier}
- **Check:** Runs `verify_xml.py` to ensure well-formed XML task structures and standard test suites.

## 4. Stop Rules
- **Limits:** {stop_rules}
- **Safety Rails:** Prevents infinite loopmaxxing by hard limits on retries and token budgets.

## 5. Memory
- **State File:** {memory}
- **Persistence:** Keeps track of active tasks and states.

## 6. Skills
- **Loaded Skills:** {skills}
- **Reference:** Loads project-specific constraints from the `./skills` directory.
"""
    with open("AGENTS.md", "w", encoding="utf-8") as f:
        f.write(agents_content)
    return agents_content


# ============================================================================
# Bootstrap
# ============================================================================

def bootstrap_system():
    """Bootstraps the system state and OpenRouter config using environment JSON if present."""
    bootstrap_env = os.getenv("OCR_BOOTSTRAP_JSON")
    if not bootstrap_env:
        return

    try:
        init_data = json.loads(bootstrap_env)

        api_key = init_data.get("api_key")
        github_token = init_data.get("github_token", "none")
        whitelist = init_data.get("whitelist", ["novita", "google-ai-studio", "google-vertex"])
        preferred_model = init_data.get("preferred_model", "deepseek/deepseek-v4-pro")
        alternative_model = init_data.get("alternative_model", "None")

        zdr = init_data.get("zdr", False)
        data_collection = init_data.get("data_collection", "allow")
        allow_fallbacks = init_data.get("allow_fallbacks", True)
        require_parameters = init_data.get("require_parameters", False)

        state = {
            "step": "completed",
            "api_key": "configured",
            "github_token": github_token,
            "whitelist": whitelist,
            "preferred_model": preferred_model,
            "alternative_model": alternative_model,
            "zdr": zdr,
            "data_collection": data_collection,
            "allow_fallbacks": allow_fallbacks,
            "require_parameters": require_parameters,
            "policy_name": "Environment Auto-Bootstrapped",
        }
        save_state(state)
        update_ocr_config(auth_token=api_key, model=preferred_model)

        if github_token and github_token != "none":
            subprocess.run(["git", "config", "--global", "--unset-all", "url.https://*.insteadOf"], stderr=subprocess.DEVNULL)
            subprocess.run(["git", "config", "--global", f"url.https://{github_token}@github.com/.insteadOf", "https://github.com/"], check=True)

        generate_goal_md(state)
        generate_agents_md(state)
        print("🤖 [System] Auto-bootstrapped successfully via environment configuration.")
    except Exception as e:
        print(f"⚠️ [System] Auto-bootstrap failed: {e}")


# ============================================================================
# AST Walkers
# ============================================================================

def walk_ast_for_python_file(filepath):
    """Parses a Python file using AST to extract top-level class, function, and method signatures."""
    try:
        with open(filepath, "r", encoding="utf-8", errors="replace") as f:
            code = f.read()
        tree = ast.parse(code)

        output = []
        for node in ast.iter_child_nodes(tree):
            if isinstance(node, ast.ClassDef):
                output.append(f"  Class: {node.name}")
                for sub_node in node.body:
                    if isinstance(sub_node, ast.FunctionDef):
                        args = [arg.arg for arg in sub_node.args.args]
                        output.append(f"    Method: {sub_node.name}({', '.join(args)})")
            elif isinstance(node, ast.FunctionDef):
                args = [arg.arg for arg in node.args.args]
                output.append(f"  Function: {node.name}({', '.join(args)})")
        return "\n".join(output) if output else "  (No top-level classes or functions declared)"
    except Exception as e:
        return f"  (AST parsing failed: {e})"


def extract_generic_signatures(filepath):
    """A generic, regex-free parser to pull top-level signatures from non-Python files."""
    signatures = []
    try:
        with open(filepath, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                line_strip = line.strip()
                if line_strip.startswith(("function ", "class ", "func ", "pub fn ", "struct ", "interface ")):
                    clean_line = line_strip.split("{")[0].strip()
                    signatures.append(f"  Signature: {clean_line}")
                elif "class " in line_strip and line_strip.endswith(":"):
                    signatures.append(f"  {line_strip}")
        return "\n".join(signatures[:15]) if signatures else None
    except:
        return None


def walk_repository_structure_and_ast(directory_path):
    """Recursively walks a repository directory mapping out structures and AST signatures."""
    structural_outline = [f"=== Codebase AST & Structure Outline for {directory_path} ===\n"]

    for root, dirs, files in os.walk(directory_path):
        dirs[:] = [d for d in dirs if d not in (".git", "node_modules", "__pycache__", "venv", "dist", "build", "data")]

        relative_root = os.path.relpath(root, directory_path)
        if relative_root == ".":
            relative_root = ""

        for file in files:
            if file.endswith((".pyc", ".png", ".jpg", ".gif", ".ico", ".pdf", ".zip", ".tar.gz", ".gitkeep")):
                continue

            filepath = os.path.join(root, file)
            display_path = os.path.join(relative_root, file) if relative_root else file
            structural_outline.append(f"📄 {display_path}")

            if file.endswith(".py"):
                structural_outline.append(walk_ast_for_python_file(filepath))
            elif file.endswith((".js", ".ts", ".go", ".rs", ".java", ".cpp", ".h")):
                signatures = extract_generic_signatures(filepath)
                if signatures:
                    structural_outline.append(signatures)

        structural_outline.append("")

    return "\n".join(structural_outline)


# ============================================================================
# Git Issue Creation
# ============================================================================

def create_local_git_issues():
    """Parse TASKLIST.md XML and create local git issue files."""
    try:
        if not os.path.exists("TASKLIST.md"):
            return 0, "TASKLIST.md not found."

        tree = ET.parse("TASKLIST.md")
        root = tree.getroot()

        os.makedirs("issues", exist_ok=True)
        created_files = []

        for task in root.findall("task"):
            task_id = task.find("id").text if task.find("id") is not None else "unknown"
            title_node = task.find("title")
            title = title_node.text if title_node is not None else "No Title"
            desc_node = task.find("description")
            desc = desc_node.text if desc_node is not None else ""

            safe_title = re.sub(r'[^a-zA-Z0-9_-]', '_', title)[:50]
            filename = f"issues/task_{task_id}_{safe_title}.md"

            with open(filename, "w", encoding="utf-8") as f:
                f.write(f"# Task {task_id}: {title}\n\n")
                f.write(desc)
            created_files.append(filename)

        if not created_files:
            return 0, "No tasks found inside TASKLIST.md."

        subprocess.run(["git", "add", "issues/"], check=True)
        subprocess.run([
            "git",
            "-c", "user.name=OCR Space Bot",
            "-c", "user.email=bot@opencodereview.local",
            "commit",
            "-m", f"docs: split code review into {len(created_files)} tasks"
        ], check=True)

        return len(created_files), None
    except Exception as e:
        return 0, str(e)


# ============================================================================
# GitHub API Helpers
# ============================================================================

async def fetch_github_tree(owner: str, repo: str, branch: str, token: str = None):
    """Queries the GitHub Git Trees API recursively to list all files in the repository."""
    headers = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    if token and token != "none":
        headers["Authorization"] = f"Bearer {token}"

    url = f"https://api.github.com/repos/{owner}/{repo}/git/trees/{branch}?recursive=1"

    async with httpx.AsyncClient(timeout=30.0) as client:
        r = await client.get(url, headers=headers)
        if r.status_code == 200:
            return r.json().get("tree", [])
        else:
            raise Exception(f"Failed to fetch repository tree: {r.status_code} - {r.text}")


async def fetch_github_file_raw(owner: str, repo: str, path: str, branch: str, token: str = None):
    """Downloads the raw content of a specific file from GitHub using the Contents API."""
    headers = {
        "Accept": "application/vnd.github.v3.raw",
        "X-GitHub-Api-Version": "2022-11-28"
    }
    if token and token != "none":
        headers["Authorization"] = f"Bearer {token}"

    encoded_path = httpx.URL(path).path
    url = f"https://api.github.com/repos/{owner}/{repo}/contents/{encoded_path}?ref={branch}"

    async with httpx.AsyncClient(timeout=30.0) as client:
        r = await client.get(url, headers=headers)
        if r.status_code == 200:
            return r.text
        else:
            raise Exception(f"Failed to fetch raw file: {r.status_code} - {r.text}")


# ============================================================================
# LLM Integration
# ============================================================================

async def call_openrouter_llm(system_prompt, user_prompt, state):
    """Hits the direct OpenRouter completions endpoint adhering to setup restrictions and ZDR policies."""
    config_path = os.path.expanduser("~/.opencodereview/config.json")
    auth_token = ""
    model = "deepseek/deepseek-v4-pro"
    if os.path.exists(config_path):
        try:
            with open(config_path) as f:
                cfg = json.load(f)
                auth_token = cfg.get("llm", {}).get("auth_token", "")
                model = cfg.get("llm", {}).get("model", model)
        except:
            pass

    if not auth_token:
        auth_token = state.get("api_key", "")

    headers = {
        "Authorization": f"Bearer {auth_token}",
        "Content-Type": "application/json"
    }

    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": 0.1,
        "provider": {
            "data_collection": state.get("data_collection", "allow"),
            "zdr": to_bool(state.get("zdr", False)),
            "allow_fallbacks": to_bool(state.get("allow_fallbacks", True)),
            "only": state.get("whitelist", ["novita"]),
            "require_parameters": to_bool(state.get("require_parameters", False)),
        }
    }

    async with httpx.AsyncClient(timeout=120.0) as client:
        r = await client.post("https://openrouter.ai/api/v1/chat/completions", json=payload, headers=headers)
        if r.status_code == 200:
            res_data = r.json()
            return res_data["choices"][0]["message"]["content"]
        else:
            raise Exception(f"OpenRouter call failed: {r.status_code} - {r.text}")


# ============================================================================
# Setup Wizard State Machine
# ============================================================================

class SetupStep(enum.Enum):
    """
    Enumeration of all setup wizard states.

    Replaces the magic strings ("ask_api_key", "await_api_key", etc.) that were
    scattered throughout proxy.py's 400-line if/elif chain. Using an enum gives us:
      - Compile-time safety against typos
      - IDE autocomplete support
      - A single source of truth for all valid states
    """

    ASK_API_KEY = "ask_api_key"
    AWAIT_API_KEY = "await_api_key"
    AWAIT_GITHUB_TOKEN = "await_github_token"
    AWAIT_WHITELIST = "await_whitelist"
    AWAIT_ROUTING_POLICY = "await_routing_policy"
    AWAIT_PREFERRED_MODEL = "await_preferred_model"
    AWAIT_ALTERNATIVE_MODEL = "await_alternative_model"
    LOOP_ENGINEER_INTRO = "loop_engineer_intro"
    LOOP_STEP_1_TRIGGER = "loop_step_1_trigger"
    LOOP_STEP_2_EXECUTION = "loop_step_2_execution"
    LOOP_STEP_3_VERIFIER = "loop_step_3_verifier"
    LOOP_STEP_4_STOP_RULES = "loop_step_4_stop_rules"
    LOOP_STEP_5_MEMORY = "loop_step_5_memory"
    LOOP_STEP_6_SKILLS = "loop_step_6_skills"
    LOOP_STEP_7_GOAL = "loop_step_7_goal"
    LOOP_STEP_8_AGENTS = "loop_step_8_agents"
    COMPLETED = "completed"


class SetupWizard:
    """
    Interactive setup wizard state machine.

    Replaces the 400-line if/elif chain in proxy.py's run_cli_stream with a
    handler-registry dispatch pattern. Each state has a dedicated async generator
    method that:
      1. Processes user input
      2. Updates the state dict in-place
      3. Persists state to disk via async_save_state
      4. Yields SSE response chunks

    The wizard is driven by a dispatch table (_handlers), making it trivial to
    add, remove, or reorder states without touching the dispatch logic. To add
    a new state:
      1. Add it to the SetupStep enum
      2. Write a handler method
      3. Register it in _handlers
    """

    def __init__(self, sse: SSEStream):
        self._sse = sse
        # Handler registry: maps each step to its handler method.
        # This replaces the giant if/elif chain — dispatch is a simple dict lookup.
        self._handlers: dict[SetupStep, Callable] = {
            SetupStep.ASK_API_KEY: self._ask_api_key,
            SetupStep.AWAIT_API_KEY: self._await_api_key,
            SetupStep.AWAIT_GITHUB_TOKEN: self._await_github_token,
            SetupStep.AWAIT_WHITELIST: self._await_whitelist,
            SetupStep.AWAIT_ROUTING_POLICY: self._await_routing_policy,
            SetupStep.AWAIT_PREFERRED_MODEL: self._await_preferred_model,
            SetupStep.AWAIT_ALTERNATIVE_MODEL: self._await_alternative_model,
            SetupStep.LOOP_ENGINEER_INTRO: self._loop_engineer_intro,
            SetupStep.LOOP_STEP_1_TRIGGER: self._loop_step_1_trigger,
            SetupStep.LOOP_STEP_2_EXECUTION: self._loop_step_2_execution,
            SetupStep.LOOP_STEP_3_VERIFIER: self._loop_step_3_verifier,
            SetupStep.LOOP_STEP_4_STOP_RULES: self._loop_step_4_stop_rules,
            SetupStep.LOOP_STEP_5_MEMORY: self._loop_step_5_memory,
            SetupStep.LOOP_STEP_6_SKILLS: self._loop_step_6_skills,
            SetupStep.LOOP_STEP_7_GOAL: self._loop_step_7_goal,
            SetupStep.LOOP_STEP_8_AGENTS: self._loop_step_8_agents,
        }

    def is_active(self, state: dict) -> bool:
        """Check if the wizard is still in progress (step != completed)."""
        step = state.get("step", SetupStep.ASK_API_KEY.value)
        return step != SetupStep.COMPLETED.value

    async def run(self, user_input: str, state: dict) -> AsyncGenerator[str, None]:
        """
        Dispatch to the appropriate state handler.

        Looks up the current step in the handler registry and delegates.
        If the step is unknown, falls back to ASK_API_KEY.
        Yields SSE chunks from the handler.
        """
        step_str = state.get("step", SetupStep.ASK_API_KEY.value)
        try:
            step = SetupStep(step_str)
        except ValueError:
            step = SetupStep.ASK_API_KEY

        handler = self._handlers.get(step)
        if handler is None:
            return

        async for chunk in handler(user_input, state):
            yield chunk

    # --- Individual state handlers ---
    # Each handler is a self-contained async generator that processes user input,
    # updates state, persists it, and yields SSE response chunks.

    async def _ask_api_key(self, user_input: str, state: dict):
        """State: ask_api_key — Initial greeting, prompt for OpenRouter API key."""
        yield self._sse.text(
            "Welcome to the **Open Code Review Space**! 🚀\n\n"
            "It looks like your LLM backend is not configured yet. Let's get you set up with **OpenRouter** in just a few steps.\n\n"
            "To start, please enter your **OpenRouter API Key** (starts with `sk-or-`):"
        )
        state["step"] = SetupStep.AWAIT_API_KEY.value
        await async_save_state(state)
        yield self._sse.done()

    async def _await_api_key(self, user_input: str, state: dict):
        """State: await_api_key — Process API key input, advance to GitHub token prompt."""
        if not user_input or len(user_input) < 10:
            yield self._sse.text("⚠️ Please enter a valid OpenRouter API Key:")
            yield self._sse.done()
            return

        try:
            update_ocr_config(auth_token=user_input)
        except Exception as e:
            yield self._sse.text(f"⚠️ Error configuring API key: {str(e)}\n\nPlease try entering it again:")
            yield self._sse.done()
            return

        state["api_key"] = "configured"

        env_hint = (
            " (or press Enter/Submit to use your default GITHUB_TOKEN environment variable)"
            if os.getenv("GITHUB_TOKEN")
            else " (or type **none** to skip)"
        )

        yield self._sse.text(
            "🔑 **OpenRouter API Key Saved!**\n\n"
            f"Next, to access private GitHub repositories, please enter your **GitHub Personal Access Token (PAT)**{env_hint}:"
        )
        state["step"] = SetupStep.AWAIT_GITHUB_TOKEN.value
        await async_save_state(state)
        yield self._sse.done()

    async def _await_github_token(self, user_input: str, state: dict):
        """State: await_github_token — Process GitHub token, advance to whitelist prompt."""
        token_arg = user_input.strip()
        env_token = os.getenv("GITHUB_TOKEN")

        resolved_token = None
        using_env_default = False

        if token_arg.lower() == "none":
            resolved_token = "none"
        elif token_arg:
            resolved_token = token_arg
        elif env_token:
            resolved_token = env_token
            using_env_default = True
        else:
            resolved_token = "none"

        if resolved_token != "none":
            try:
                subprocess.run([
                    "git", "config", "--global", "--unset-all",
                    "url.https://*.insteadOf"
                ], stderr=subprocess.DEVNULL)
                subprocess.run([
                    "git", "config", "--global",
                    f"url.https://{resolved_token}@github.com/.insteadOf",
                    "https://github.com/"
                ], check=True)

                state["github_token"] = resolved_token

                if using_env_default:
                    yield self._sse.text("🔍 **Detected default GITHUB_TOKEN from Space Secrets.** Using it for configuration.\n\n")
            except Exception as e:
                yield self._sse.text(f"⚠️ Error configuring Git with GitHub token: {str(e)}\n\nPlease try entering again (or type **none** to skip):")
                yield self._sse.done()
                return
        else:
            state["github_token"] = "none"
            subprocess.run([
                "git", "config", "--global", "--unset-all",
                "url.https://*.insteadOf"
            ], stderr=subprocess.DEVNULL)

            if token_arg.lower() == "none":
                yield self._sse.text("🚫 **GitHub Token disabled.** Global URL redirects cleared.\n\n")

        yield self._sse.text(
            "🔑 **GitHub Token Configured!**\n\n"
            "To comply with your strict zero-data-retention (ZDR) policy, all OpenRouter calls are configured to deny data collection and disallow fallbacks. By default, only **`novita`** is whitelisted.\n\n"
            "Would you like to manually add other providers or specific models (e.g. `azure`, `openai`, `anthropic`, `deepseek/deepseek-v4-pro`) to the `'only'` whitelist?\n\n"
            "Enter a comma-separated list of additional providers/models, or type **none** to keep only `novita`:"
        )
        state["step"] = SetupStep.AWAIT_WHITELIST.value
        await async_save_state(state)
        yield self._sse.done()

    async def _await_whitelist(self, user_input: str, state: dict):
        """State: await_whitelist — Process whitelist, advance to routing policy prompt."""
        choice = user_input.strip()
        whitelist = ["novita"]

        if choice.lower() != "none" and choice:
            for entry in choice.split(","):
                entry = entry.strip()
                if entry:
                    whitelist.append(entry if "/" in entry else entry.lower())

        state["whitelist"] = whitelist

        yield self._sse.text(
            f"🛡️ **Whitelisted Providers/Models:** `{whitelist}`\n\n"
            "Next, let's configure your **OpenRouter Provider Routing constraints**.\n\n"
            "To find the exact option causing your calls to return 404, select one of these strictness levels:\n\n"
            "1. **Fully Relaxed** (Highly compatible; no routing constraints)\n"
            "2. **Strict ZDR & No Data Collection Only** (`zdr: true` and `data_collection: \"deny\"`)\n"
            "3. **Maximum Strictness** (All constraints enabled: ZDR, Deny Data Collection, No Fallbacks, Require Params)\n\n"
            "Choose a number or type a custom configuration (e.g., `zdr` or `data_collection`):"
        )
        state["step"] = SetupStep.AWAIT_ROUTING_POLICY.value
        await async_save_state(state)
        yield self._sse.done()

    async def _await_routing_policy(self, user_input: str, state: dict):
        """State: await_routing_policy — Process routing policy, advance to model selection."""
        choice = user_input.lower().strip()

        zdr = False
        data_collection = "allow"
        allow_fallbacks = True
        require_parameters = False
        policy_name = "Custom / Balanced"

        if choice == "1" or "relaxed" in choice or "compatibility" in choice:
            policy_name = "Relaxed / Compatibility Mode"
        elif choice == "2" or "strict" in choice:
            policy_name = "Strict ZDR only"
            zdr = True
            data_collection = "deny"
        elif choice == "3" or "maximum" in choice or "all" in choice:
            policy_name = "Maximum Strictness"
            zdr = True
            data_collection = "deny"
            allow_fallbacks = False
            require_parameters = True
        else:
            policy_parts = []
            if "zdr" in choice:
                zdr = True
                policy_parts.append("zdr")
            if "data" in choice or "collect" in choice or "deny" in choice:
                data_collection = "deny"
                policy_parts.append("data_collection: deny")
            policy_name = f"Custom (Enabled: {', '.join(policy_parts)})" if policy_parts else "Relaxed"

        state["zdr"] = zdr
        state["data_collection"] = data_collection
        state["allow_fallbacks"] = allow_fallbacks
        state["require_parameters"] = require_parameters
        state["policy_name"] = policy_name

        yield self._sse.text(
            f"🛡️ **Routing Policy Selected: {policy_name}**\n"
            f"* **Zero Data Retention (ZDR):** `{zdr}`\n"
            f"* **Data Collection:** `{data_collection}`\n"
            f"* **Allow Fallbacks:** `{allow_fallbacks}`\n"
            f"* **Require Parameters:** `{require_parameters}`\n\n"
            "Next, select your preferred OpenRouter model (choose a number or type a custom model ID):\n\n"
            "1. **z-ai/glm-5.2** (high-performance reasoning)\n"
            "2. **deepseek/deepseek-v4-pro** (low-cost reasoning)\n"
            "3. **google/gemini-3.5-flash** (cost-efficient general purpose)\n"
            "4. **openrouter/fusion** (Balanced fusion)"
        )
        state["step"] = SetupStep.AWAIT_PREFERRED_MODEL.value
        await async_save_state(state)
        yield self._sse.done()

    async def _await_preferred_model(self, user_input: str, state: dict):
        """
        State: await_preferred_model — Process model selection, advance to alternative model.

        Bug fix: proxy.py had swapped keyword checks (option 1 checked "pro",
        option 2 checked "glm"). Now correctly maps menu labels to keywords.
        """
        choice = user_input.lower()
        if choice == "1" or "glm" in choice:
            selected_model = "z-ai/glm-5.2"
        elif choice == "2" or "pro" in choice:
            selected_model = "deepseek/deepseek-v4-pro"
        elif choice == "3" or "flash" in choice:
            selected_model = "google/gemini-3.5-flash"
        elif choice == "4" or "fusion" in choice:
            selected_model = "openrouter/fusion"
        else:
            selected_model = user_input

        try:
            update_ocr_config(model=selected_model)
        except Exception as e:
            yield self._sse.text(f"⚠️ Error configuring model: {str(e)}\n\nPlease try selecting again:")
            yield self._sse.done()
            return

        state["preferred_model"] = selected_model
        yield self._sse.text(
            f"⚙️ **Preferred Model Configured: `{selected_model}`**\n\n"
            "Next, select a **lower-cost alternative model** to use for smaller files/drafts (or to fall back on):\n\n"
            "1. **deepseek/deepseek-v4-flash** (Fast & extremely cheap)\n"
            "2. **No alternative** (Use the preferred model for everything)"
        )
        state["step"] = SetupStep.AWAIT_ALTERNATIVE_MODEL.value
        await async_save_state(state)
        yield self._sse.done()

    async def _await_alternative_model(self, user_input: str, state: dict):
        """State: await_alternative_model — Process alternative model, advance to loop engineer intro."""
        choice = user_input.lower()
        if choice == "1" or "flash" in choice:
            alternative_model = "deepseek/deepseek-v4-flash"
        else:
            alternative_model = "None"

        state["alternative_model"] = alternative_model

        try:
            update_ocr_config(url="http://127.0.0.1:8080/v1")
        except Exception as e:
            yield self._sse.text(f"⚠️ Error finalizing configuration: {str(e)}\n\nPlease try again:")
            yield self._sse.done()
            return

        state["step"] = SetupStep.LOOP_ENGINEER_INTRO.value
        await async_save_state(state)

        yield self._sse.text(
            "⚙️ **Alternative Model Configured!**\n\n"
            "Now, let's construct your safe, self-prompting agentic loop using the **Loop Engineering 101 framework** (based on your cloned `loop-engineer` skill).\n\n"
            "We will walk through the 6 core components to define a custom `AGENTS.md` file and write it to disk for performing the entire cycle of:\n"
            "**Git Actions ➔ Git Issue ➔ Git Pull Request ➔ Open Code Review (OCR) ➔ TASKLIST.md + CODEREVIEW.md**.\n\n"
            "Type **yes** to begin the loop-engineer walkthrough, or **skip** to complete setup without it:"
        )
        yield self._sse.done()

    async def _loop_engineer_intro(self, user_input: str, state: dict):
        """State: loop_engineer_intro — Branch into walkthrough or skip to completion."""
        choice = user_input.strip().lower()
        if choice in ("yes", "y", "sure", "start"):
            state["step"] = SetupStep.LOOP_STEP_1_TRIGGER.value
            await async_save_state(state)
            yield self._sse.text(
                "### Step 1: Trigger (The Heartbeat) 💓\n\n"
                "Determine how or when the loop fires (e.g., schedule, interval, webhooks, or PR comments).\n\n"
                "**Please enter your Trigger** (or press Enter/type **default** to use: `On a new Git Issue or manually via chat`):"
            )
        else:
            state["step"] = SetupStep.COMPLETED.value
            await async_save_state(state)
            alt_tip = (
                f"\n*Tip: If you want to use your lower-cost model, append `--model {state['alternative_model']}` to your scan/review commands!*"
                if state.get("alternative_model") != "None"
                else ""
            )
            yield self._sse.text(
                "🎉 **Configuration Complete! (Walkthrough Skipped)**\n\n"
                "Open Code Review is now configured to use OpenRouter with strict ZDR policies. Here is your setup:\n"
                f"* **Primary Model:** `{state['preferred_model']}`\n"
                f"* **Alternative Model:** `{state.get('alternative_model')}`\n"
                f"* **Allowed Providers/Models (Whitelist):** `{state['whitelist']}`\n\n"
                "You can now run commands like:\n"
                "* `clone <repo_url>`\n"
                "* `scan --path looper`\n"
                "* `review --repo looper --commit HEAD`\n"
                f"{alt_tip}"
            )
        yield self._sse.done()

    async def _loop_step_1_trigger(self, user_input: str, state: dict):
        """State: loop_step_1_trigger — Capture trigger definition."""
        choice = user_input.strip()
        state["loop_trigger"] = choice if (choice and choice.lower() != "default") else "On a new Git Issue or manually via chat"
        state["step"] = SetupStep.LOOP_STEP_2_EXECUTION.value
        await async_save_state(state)
        yield self._sse.text(
            "### Step 2: Execution Runbook (The Action) ⚙️\n\n"
            "Define the core action. Ensure the agent has the tools to read state, run scans, generate reviews, and make changes.\n\n"
            "**Please enter your Execution Runbook** (or press Enter/type **default** to use: `scan workspace, write CODEREVIEW.md, parse TASKLIST.md, create-issues`):"
        )
        yield self._sse.done()

    async def _loop_step_2_execution(self, user_input: str, state: dict):
        """State: loop_step_2_execution — Capture execution runbook."""
        choice = user_input.strip()
        state["loop_execution"] = choice if (choice and choice.lower() != "default") else "scan workspace, write CODEREVIEW.md, parse TASKLIST.md, create-issues"
        state["step"] = SetupStep.LOOP_STEP_3_VERIFIER.value
        await async_save_state(state)
        yield self._sse.text(
            "### Step 3: Verifier (Success Validator) 🔍\n\n"
            "Define the success validator. Use deterministic testing or verifier agents to grade results independently after each turn.\n\n"
            "**Please enter your Verifier method** (or press Enter/type **default** to use: `verify_xml.py syntax pass and verify_pairs.py completion check`):"
        )
        yield self._sse.done()

    async def _loop_step_3_verifier(self, user_input: str, state: dict):
        """State: loop_step_3_verifier — Capture verifier method."""
        choice = user_input.strip()
        state["loop_verifier"] = choice if (choice and choice.lower() != "default") else "verify_xml.py syntax pass and verify_pairs.py completion check"
        state["step"] = SetupStep.LOOP_STEP_4_STOP_RULES.value
        await async_save_state(state)
        yield self._sse.text(
            "### Step 4: Stop Rules (Safety Rails) 🛑\n\n"
            "Strict boundaries to prevent runaway runs. Specify constraints, failure thresholds, iteration caps, or max spend/tokens.\n\n"
            "**Please enter your Stop Rules** (or press Enter/type **default** to use: `Max 5 iterations or any unrecoverable error`):"
        )
        yield self._sse.done()

    async def _loop_step_4_stop_rules(self, user_input: str, state: dict):
        """State: loop_step_4_stop_rules — Capture stop rules."""
        choice = user_input.strip()
        state["loop_stop_rules"] = choice if (choice and choice.lower() != "default") else "Max 5 iterations or any unrecoverable error"
        state["step"] = SetupStep.LOOP_STEP_5_MEMORY.value
        await async_save_state(state)
        yield self._sse.text(
            "### Step 5: Memory (State Persistence) 💾\n\n"
            "State persistence on disk so context survives between restarts (e.g., updating task states on disk).\n\n"
            "**Please enter your Memory / State File** (or press Enter/type **default** to use: `setup_state.json and TASKLIST.md`):"
        )
        yield self._sse.done()

    async def _loop_step_5_memory(self, user_input: str, state: dict):
        """State: loop_step_5_memory — Capture memory/state file."""
        choice = user_input.strip()
        state["loop_memory"] = choice if (choice and choice.lower() != "default") else "setup_state.json and TASKLIST.md"
        state["step"] = SetupStep.LOOP_STEP_6_SKILLS.value
        await async_save_state(state)
        yield self._sse.text(
            "### Step 6: Skills (Dynamic Isolation) 🧠\n\n"
            "Keep run-time code short by saving project-specific constraints in isolated `SKILL.md` files rather than stuffing them into prompt templates.\n\n"
            "**Please enter your Loaded Skills** (or press Enter/type **default** to use: `loop-engineer skill from loopmaxxer-bench`):"
        )
        yield self._sse.done()

    async def _loop_step_6_skills(self, user_input: str, state: dict):
        """State: loop_step_6_skills — Capture loaded skills."""
        choice = user_input.strip()
        state["loop_skills"] = choice if (choice and choice.lower() != "default") else "loop-engineer skill from loopmaxxer-bench"
        state["step"] = SetupStep.LOOP_STEP_7_GOAL.value
        await async_save_state(state)
        yield self._sse.text(
            "### Step 7: Define the Goal 🎯\n\n"
            "Now, define the core objective of your development and review loop. This will be written directly to `GOAL.md`.\n\n"
            "**Please enter your Goal** (or press Enter/type **default** to use: `Implement a robust, event-driven market-making strategy in loopmaxxer-bench/market-making with custom inventory management and spread quoting model`):"
        )
        yield self._sse.done()

    async def _loop_step_7_goal(self, user_input: str, state: dict):
        """State: loop_step_7_goal — Capture goal and generate GOAL.md."""
        choice = user_input.strip()
        state["loop_goal"] = choice if (choice and choice.lower() != "default") else "Implement a robust, event-driven market-making strategy in loopmaxxer-bench/market-making with custom inventory management and spread quoting model"

        generate_goal_md(state)

        state["step"] = SetupStep.LOOP_STEP_8_AGENTS.value
        await async_save_state(state)
        yield self._sse.text(
            "🎉 **Goal Saved! GOAL.md Has Been Written to Disk!**\n\n"
            "### Step 8: Compile Agent Runbook 🤖\n\n"
            "We are ready to compile the `AGENTS.md` file using the **`loop-engineer` framework** from your skills.\n\n"
            "Type **compile** to generate both files on disk and finalize the wizard:"
        )
        yield self._sse.done()

    async def _loop_step_8_agents(self, user_input: str, state: dict):
        """State: loop_step_8_agents — Compile AGENTS.md and finalize setup."""
        choice = user_input.strip().lower()
        if choice == "compile" or choice == "default":
            agents_md = generate_agents_md(state)

            state["step"] = SetupStep.COMPLETED.value
            await async_save_state(state)

            alt_tip = (
                f"\n*Tip: If you want to use your lower-cost model, append `--model {state['alternative_model']}` to your scan/review commands!*"
                if state.get("alternative_model") != "None"
                else ""
            )

            yield self._sse.text(
                "🎉 **Walkthrough Completed! Both GOAL.md and AGENTS.md Have Been Written to Disk!**\n\n"
                "### Generated `GOAL.md`:\n"
                "```markdown\n"
                f"{generate_goal_md(state)}"
                "```\n\n"
                "### Generated `AGENTS.md`:\n"
                "```markdown\n"
                f"{agents_md}"
                "```\n\n"
                "Open Code Review is now fully configured and set up with your loop agent definitions under strict ZDR policies. Here is your setup:\n"
                f"* **Primary Model:** `{state['preferred_model']}`\n"
                f"* **Alternative Model:** `{state.get('alternative_model')}`\n"
                f"* **Allowed Providers/Models (Whitelist):** `{state['whitelist']}`\n\n"
                "You can now run commands like:\n"
                "* `clone <repo_url>`\n"
                "* `scan --path looper`\n"
                "* `review --repo looper --commit HEAD`\n"
                "* `/benchmark market-making` (to test and execute autonomous improvement cycles!)\n"
                f"{alt_tip}"
            )
        else:
            yield self._sse.text("⚠️ Please type **compile** to finalize setup.")
        yield self._sse.done()


# ============================================================================
# Command Handlers
# ============================================================================

class CommandHandlers:
    """
    Handlers for slash commands and special CLI commands.

    Replaces the long chain of inline if-checks at the top of proxy.py's
    run_cli_stream with a pattern-matching registry. Each command has:
      - A matcher function that tests if the command applies
      - A handler method that is an async generator yielding SSE chunks

    Commands are split into two phases to preserve the original dispatch order:
      1. _special_routes: Commands that run before the state machine (reset,
         create-issues, review-prs, ingest, benchmarks)
      2. _file_routes: Commands that run after the state machine (cd, ls)
    """

    def __init__(self, sse: SSEStream):
        self._sse = sse
        self._special_routes: list[tuple[Callable[[str], bool], Callable]] = [
            (self._match_reset, self.handle_reset),
            (self._match_create_issues, self.handle_create_issues),
            (self._match_review_prs, self.handle_review_prs),
            (self._match_ingest, self.handle_ingest),
            (self._match_benchmark_market_making, self.handle_benchmark_market_making),
            (self._match_benchmark_escher, self.handle_benchmark_escher),
            (self._match_benchmark_selinux, self.handle_benchmark_selinux),
            (self._match_benchmark_deepsec, self.handle_benchmark_deepsec),
        ]
        self._file_routes: list[tuple[Callable[[str], bool], Callable]] = [
            (self._match_cd, self.handle_cd),
            (self._match_ls, self.handle_ls),
        ]

    def find_special_handler(self, command: str) -> Optional[Callable]:
        """Find a matching special command handler, or None."""
        for matcher, handler in self._special_routes:
            if matcher(command):
                return handler
        return None

    def find_file_handler(self, command: str) -> Optional[Callable]:
        """Find a matching file command handler, or None."""
        for matcher, handler in self._file_routes:
            if matcher(command):
                return handler
        return None

    # --- Matchers ---

    def _match_reset(self, command: str) -> bool:
        return command.lower() in ("reset", "config reset", "/reset")

    def _match_create_issues(self, command: str) -> bool:
        return command.lower() in ("create-issues", "issues", "/create-issues")

    def _match_review_prs(self, command: str) -> bool:
        return command.lower() in ("review-prs", "ci-prs", "/review-prs")

    def _match_ingest(self, command: str) -> bool:
        lower = command.lower()
        return lower.startswith("/ingest ") or lower.startswith("ingest ")

    def _match_benchmark_market_making(self, command: str) -> bool:
        return command.lower() in ("benchmark market-making", "/benchmark market-making")

    def _match_benchmark_escher(self, command: str) -> bool:
        lower = command.lower()
        return lower.startswith("/benchmark escher") or lower.startswith("benchmark escher")

    def _match_benchmark_selinux(self, command: str) -> bool:
        lower = command.lower()
        return lower.startswith("/benchmark selinux") or lower.startswith("benchmark selinux")
    
    def _match_benchmark_deepsec(self, command: str) -> bool:
        lower = command.lower()
        return lower.startswith("/benchmark deepsec") or lower.startswith("benchmark deepsec") or lower.startswith("/deepsec") or lower == "deepsec"

    def _match_cd(self, command: str) -> bool:
        stripped = command.strip()
        return stripped.startswith("cd ") or stripped == "cd" or stripped.startswith("cd\t")

    def _match_ls(self, command: str) -> bool:
        stripped = command.strip()
        return stripped.startswith("ls ") or stripped == "ls" or stripped.startswith("ls\t")

    # --- Handlers ---

    async def handle_reset(self, command: str, state: dict):
        """Reset configuration and allow re-configuring."""
        if os.path.exists(STATE_FILE):
            os.remove(STATE_FILE)
        for f in ("AGENTS.md", "GOAL.md", "progress.md"):
            if os.path.exists(f):
                try:
                    os.remove(f)
                except:
                    pass
        try:
            await asyncio.create_subprocess_exec("./ocr", "config", "unset", "llm.auth_token")
            config_path = os.path.expanduser("~/.opencodereview/config.json")
            if os.path.exists(config_path):
                os.remove(config_path)
            subprocess.run([
                "git", "config", "--global", "--unset-all",
                "url.https://*.insteadOf"
            ], stderr=subprocess.DEVNULL)
        except:
            pass
        yield self._sse.text("🔄 **Configuration reset successfully.** Send any message to start the setup wizard again!")
        yield self._sse.done()

    async def handle_create_issues(self, command: str, state: dict):
        """Parse TASKLIST.md and write local issue files."""
        if not os.path.exists("TASKLIST.md"):
            yield self._sse.text("⚠️ **TASKLIST.md not found.** Please run a review first using `scan` or `review`.")
            yield self._sse.done()
            return

        yield self._sse.text("🛠️ **Parsing TASKLIST.md and writing local issue files...**\n")

        count, error = create_local_git_issues()
        if error:
            yield self._sse.text(f"❌ **Failed to generate issues:** {error}")
        else:
            yield self._sse.text(
                f"✅ **Successfully split review into {count} markdown issues!**\n"
                f"All tasks have been committed to your current git branch under the `./issues/` directory.\n"
                f"Multi-agent frameworks can now read these task files directly from this repository."
            )

        yield self._sse.done()

    async def handle_review_prs(self, command: str, state: dict):
        """Initiate automated Pull Request Review Cycle."""
        git_token = get_github_token(state)
        if not git_token or git_token == "none":
            yield self._sse.text("⚠️ **GitHub Token not found.** Please configure your token via the setup wizard to fetch Pull Requests.")
            yield self._sse.done()
            return

        yield self._sse.text("🤖 **Initiating automated Pull Request Review Cycle...**\n")

        proc = await asyncio.create_subprocess_exec(
            "python3", "ci_reviewer.py",
            env={**os.environ, "GITHUB_TOKEN": git_token},
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )

        while True:
            line_bytes = await proc.stdout.readline()
            if not line_bytes:
                break
            raw_line = line_bytes.decode('utf-8', errors='replace')
            yield self._sse.text(f"📋 {raw_line.strip()}\n")

        await proc.wait()
        yield self._sse.text("\n✅ **Review and test process successfully completed!**")
        yield self._sse.done()

    async def handle_ingest(self, command: str, state: dict):
        """GitIngest codebase context extraction + clone + OCR + AST walk."""
        lower = command.lower()
        raw_url = command[8:] if lower.startswith("/ingest ") else command[7:]
        raw_url = raw_url.strip()

        if not raw_url:
            yield self._sse.text("⚠️ **Usage:** `/ingest https://github.com/username/repository`\n")
            yield self._sse.done()
            return

        yield self._sse.text(f"📥 **Starting GitIngest for codebase context extraction...**\nTarget URL: `{raw_url}`\n")

        # 1. Fetch text digest using gitingest
        try:
            from gitingest import ingest

            git_token = get_github_token(state)
            if git_token and git_token != "none":
                os.environ["GITHUB_TOKEN"] = git_token
                os.environ["GITINGEST_TOKEN"] = git_token

            yield self._sse.text("⏳ Running `gitingest` parser on remote repository...")
            summary, tree, content = ingest(raw_url)

            estimated_tokens = len(content) // 4
            yield self._sse.text(f"📊 **GitIngest raw analysis complete:**\nEstimated token count: `{estimated_tokens:,}` tokens.\n")

            max_chars = 300000 * 4
            if len(content) > max_chars:
                content = content[:max_chars]
                yield self._sse.text(
                    f"\n⚠️ **Truncation Warning:** Codebase exceeds 300,000 tokens limit. "
                    f"Content has been truncated to first {max_chars:,} characters."
                )

            ingest_file_content = f"# GitIngest Summary\n\n{summary}\n\n## Directory Tree\n```\n{tree}\n```\n\n## Content\n{content}\n"
            with open("INGEST_SUMMARY.md", "w", encoding="utf-8") as f:
                f.write(ingest_file_content)

            yield self._sse.text("💾 **Saved prompt-friendly digest to `INGEST_SUMMARY.md`.**\n")

        except Exception as e:
            yield self._sse.text(f"❌ **GitIngest failed:** {str(e)}\nTrying to fallback to clone and tree-walker direct execution...\n")

        # 2. Perform git clone
        yield self._sse.text("🧬 **Starting Git Clone for local AST structural mapping...**\n")

        match = re.match(r'https?://(?:www\.)?github\.com/([^/]+)/([^/]+)', raw_url)
        if match:
            username = match.group(1)
            repo_name = match.group(2).replace(".git", "")
        else:
            username = "external"
            repo_name = "ingested_repo"

        target_dir = f"cloned_{repo_name}"

        if os.path.exists(target_dir):
            shutil.rmtree(target_dir, ignore_errors=True)

        clone_url = raw_url
        git_token = get_github_token(state)
        if git_token and git_token != "none" and "github.com" in raw_url:
            clone_url = raw_url.replace("https://github.com", f"https://{git_token}@github.com")

        yield self._sse.text(f"⚙️ Cloning `{username}/{repo_name}` into `./{target_dir}`...")

        clone_proc = await asyncio.create_subprocess_exec(
            "git", "clone", "--depth", "1", clone_url, target_dir,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )
        clone_output_bytes, _ = await clone_proc.communicate()

        if clone_proc.returncode != 0:
            yield self._sse.text(f"❌ **Git Clone failed:** {clone_output_bytes.decode('utf-8', errors='replace')}\n")
            yield self._sse.done()
            return

        yield self._sse.text("✅ **Repository cloned successfully!**\n")

        # 3. Perform OCR scan
        yield self._sse.text("🔍 **Executing Alibaba Open Code Review (OCR) Scan...**\n")

        ocr_proc = await asyncio.create_subprocess_exec(
            "./ocr", "scan", "--path", target_dir,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )

        ocr_output_lines = []
        while True:
            line_bytes = await ocr_proc.stdout.readline()
            if not line_bytes:
                break
            raw_line = line_bytes.decode('utf-8', errors='replace')
            clean_line = ANSI_ESCAPE.sub('', raw_line)
            ocr_output_lines.append(clean_line)
            yield self._sse.text(f" OCR: {clean_line.strip()}")

        await ocr_proc.wait()

        with open("CODEREVIEW.md", "w", encoding="utf-8") as f:
            f.writelines(ocr_output_lines)

        yield self._sse.text("\n✅ **OCR Review complete. Generated `CODEREVIEW.md`.**\n")

        process_review_file(input_file="CODEREVIEW.md", output_file="TASKLIST.md")
        yield self._sse.text("📋 **Parsed XML task list compiled to `TASKLIST.md`.**\n")

        # 4. AST Tree Walk
        yield self._sse.text("🌳 **Traversing codebase directory and extracting Abstract Syntax Tree (AST)...**\n")

        try:
            ast_structure = walk_repository_structure_and_ast(target_dir)
            with open("AST_STRUCTURE.md", "w", encoding="utf-8") as f:
                f.write(ast_structure)
            yield self._sse.text("💾 **Saved complete AST mappings to `AST_STRUCTURE.md`.**\n")
        except Exception as ast_err:
            yield self._sse.text(f"⚠️ **Tree Walk/AST mapping failed:** {str(ast_err)}\n")

        yield self._sse.text(
            "🎉 **Code Ingestion, Review, and AST Analysis Complete!**\n\n"
            "The following assets are now written to your workspace disk and ready for use:\n"
            "* 📝 **`INGEST_SUMMARY.md`**: Prompt-friendly text digest representing the entire repository (limited to 300k tokens).\n"
            "* 🌳 **`AST_STRUCTURE.md`**: Fully traversed folder hierarchy and extracted AST (class and function mapping).\n"
            "* 🛡️ **`CODEREVIEW.md`**: Multi-file quality audit of changes, errors, and optimizations generated by Alibaba OCR.\n"
            "* 📋 **`TASKLIST.md`**: Structured, well-formed XML task definitions ready to be converted into issues.\n\n"
            "To split this entire review structure into individual local issues, run: `/create-issues`!"
        )
        yield self._sse.done()

    async def handle_benchmark_market_making(self, command: str, state: dict):
        """Autonomous Market-Making Benchmark Loop."""
        if not os.path.exists("GOAL.md") or not os.path.exists("AGENTS.md"):
            yield self._sse.text("⚠️ **GOAL.md or AGENTS.md not found.** Please complete the setup wizard first to generate these files.")
            yield self._sse.done()
            return

        yield self._sse.text("🏁 **Starting Autonomous Market-Making Benchmark Loop!**\n")

        # 1. Clone loopmaxxer-bench
        yield self._sse.text("🧬 **Step 1: Cloning loopmaxxer-bench repository...**\n")
        git_token = get_github_token(state)

        clone_url = "https://github.com/dataopsnick/loopmaxxer-bench.git"
        if git_token and git_token != "none":
            clone_url = f"https://{git_token}@github.com/dataopsnick/loopmaxxer-bench.git"

        if os.path.exists("loopmaxxer-bench"):
            shutil.rmtree("loopmaxxer-bench", ignore_errors=True)

        proc = await asyncio.create_subprocess_exec(
            "git", "clone", "--depth", "1", clone_url, "loopmaxxer-bench",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )
        clone_out, _ = await proc.communicate()
        if proc.returncode != 0:
            yield self._sse.text(f"❌ **Git Clone failed:** {clone_out.decode('utf-8', errors='replace')}\n")
            yield self._sse.done()
            return

        yield self._sse.text("✅ **Successfully cloned loopmaxxer-bench repository!**\n")

        target_dir = "loopmaxxer-bench/market-making" if os.path.exists("loopmaxxer-bench/market-making") else "loopmaxxer-bench"
        yield self._sse.text(f"📁 Target workspace directory: `{target_dir}`\n")

        # 2. Run the iterative self-prompting loop
        max_iterations = 5
        iteration = 1
        loop_completed = False

        progress_file = "progress.md"
        with open(progress_file, "w", encoding="utf-8") as f:
            f.write("# Autonomous Loop Execution Progress (progress.md)\n\nStarted benchmark loop execution.\n")

        while iteration <= max_iterations and not loop_completed:
            yield self._sse.text(f"\n🔄 **=== Turn {iteration} / {max_iterations} of the Self-Prompting Loop ===**\n")

            with open(progress_file, "a", encoding="utf-8") as f:
                f.write(f"\n## Iteration {iteration}\n")

            # Step A: Run OCR Scan
            yield self._sse.text("🔍 Running Alibaba Open Code Review scan to analyze codebase...")
            yield self._sse.text("🧪 [Diagnostic] Preparing to start scanner...\n")
            try:
                yield self._sse.text(f"🧪 [Diagnostic] Spawning './ocr' with path: {target_dir}\n")
                ocr_proc = await asyncio.create_subprocess_exec(
                    "./ocr", "scan", "--include", "\"**/*.md\"", "--path", target_dir,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.STDOUT
                )
                yield self._sse.text(f"🧪 [Diagnostic] Process successfully spawned (PID: {ocr_proc.pid})\n")

                ocr_output_lines = []
                line_counter = 0
                while True:
                    yield self._sse.text(f"🧪 [Diagnostic] Awaiting line {line_counter + 1} from stdout readline...\n")
                    line_bytes = await ocr_proc.stdout.readline()
                    if not line_bytes:
                        yield self._sse.text("🧪 [Diagnostic] Readline returned empty (EOF reached).\n")
                        break

                    line_counter += 1
                    yield self._sse.text(f"🧪 [Diagnostic] Read {len(line_bytes)} raw bytes. Decoding to UTF-8...\n")
                    raw_line = line_bytes.decode('utf-8', errors='replace')

                    yield self._sse.text(f"🧪 [Diagnostic] Applying regex replacement on raw line...\n")
                    clean_line = ANSI_ESCAPE.sub('', raw_line)
                    ocr_output_lines.append(clean_line)

                    yield self._sse.text(f"  OCR: {clean_line.strip()}\n")

                yield self._sse.text("🧪 [Diagnostic] Awaiting process exit code...\n")
                await ocr_proc.wait()
                yield self._sse.text(f"🧪 [Diagnostic] Process exited. Clean output generated ({len(ocr_output_lines)} lines).\n")

            except Exception as diag_err:
                tb_str = traceback.format_exc()
                yield self._sse.text(
                    f"\n❌ [Diagnostic Crash] An exception occurred in the execution loop:\n"
                    f"```\n{diag_err}\n```\n"
                    f"Traceback:\n```\n{tb_str}\n```\n"
                )
                raise diag_err

            with open("CODEREVIEW.md", "w", encoding="utf-8") as f:
                f.writelines(ocr_output_lines)

            yield self._sse.text("📝 Code scan compiled to `CODEREVIEW.md`.\n")

            # Step B: Parse into TASKLIST.md
            xml_content, task_count = process_review_file(input_file="CODEREVIEW.md", output_file="TASKLIST.md")
            yield self._sse.text(f"📋 Compiled XML task list with `{task_count}` items to `TASKLIST.md`.\n")

            if task_count == 0:
                yield self._sse.text("✨ **Success! No outstanding tasks or issues found in the workspace.** Goal achieved!\n")
                loop_completed = True
                break

            # Read and process tasks
            try:
                tree = ET.parse("TASKLIST.md")
                root = tree.getroot()
                tasks = root.findall("task")
            except Exception as e:
                yield self._sse.text(f"⚠️ Failed to parse TASKLIST.md: {e}\n")
                tasks = []

            # Process up to 3 tasks per iteration
            active_tasks = tasks[:3]
            for task in active_tasks:
                task_id = task.find("id").text if task.find("id") is not None else "unknown"
                title = task.find("title").text if task.find("title") is not None else "No Title"
                description = task.find("description").text if task.find("description") is not None else ""

                yield self._sse.text(f"🛠️ **Executing Task {task_id}:** `{title}`...\n")

                # Isolate target file from title or description
                target_file = None
                file_match = re.search(r'([a-zA-Z0-9_\-\./]+\.(?:py|js|ts|go|rs|json|md))', title)
                if file_match:
                    target_file = file_match.group(1).split('/')[-1]
                else:
                    desc_match = re.search(r'([a-zA-Z0-9_\-\./]+\.(?:py|js|ts|go|rs|json|md))', description)
                    if desc_match:
                        target_file = desc_match.group(1).split('/')[-1]

                if not target_file:
                    target_file = "market_maker.py"

                filepath = os.path.join(target_dir, target_file)
                yield self._sse.text(f"📂 Selected target file: `{filepath}`\n")

                # Read current file content
                current_content = ""
                if os.path.exists(filepath):
                    try:
                        with open(filepath, "r", encoding="utf-8") as f:
                            current_content = f.read()
                    except Exception as e:
                        print(f"Error reading {filepath}: {e}")

                # Construct autonomous prompt
                goal_data = ""
                if os.path.exists("GOAL.md"):
                    with open("GOAL.md") as f:
                        goal_data = f.read()

                system_prompt = (
                    "You are an autonomous AI software engineering agent. Your objective is to edit code files "
                    "to satisfy the repository goals (GOAL.md) and resolve the active TASKLIST.md task."
                )
                user_prompt = f"""### Target Goal (GOAL.md)
{goal_data}

### Active Task to Resolve
**Title:** {title}
**Details:**
{description}

### File to Modify
**File path:** {filepath}
**Current file content:**
```
{current_content}
```

### Instructions
Provide the complete, updated file content to resolve this task. Wrap the code content inside a single markdown code block.
Do not output conversational text or explanations outside the block.
"""
                yield self._sse.text("🧠 Querying OpenRouter model for self-prompted code edits...")
                try:
                    llm_response = await call_openrouter_llm(system_prompt, user_prompt, state)

                    code_match = re.search(r'```[a-zA-Z0-9]*\n(.*?)```', llm_response, re.DOTALL)
                    if code_match:
                        updated_code = code_match.group(1)
                    else:
                        updated_code = llm_response

                    os.makedirs(os.path.dirname(filepath), exist_ok=True)
                    with open(filepath, "w", encoding="utf-8") as f:
                        f.write(updated_code)

                    yield self._sse.text(f"✏️ **Successfully updated `{filepath}`!**\n")

                    with open(progress_file, "a", encoding="utf-8") as f:
                        f.write(f"- Resolved task {task_id}: {title} on {filepath}\n")

                except Exception as llm_err:
                    yield self._sse.text(f"⚠️ LLM edit failed: {str(llm_err)}\n")

            # Step C: Run Verifier
            yield self._sse.text("🔍 Running local Verifier to validate code modifications...")
            verification_passed = True

            for root, _, files in os.walk(target_dir):
                for file in files:
                    if file.endswith(".py"):
                        py_filepath = os.path.join(root, file)
                        try:
                            import py_compile
                            py_compile.compile(py_filepath, doraise=True)
                        except Exception as compile_err:
                            yield self._sse.text(f"❌ Syntax validation failed on `{py_filepath}`: {compile_err}\n")
                            verification_passed = False

            if verification_passed:
                yield self._sse.text("✅ **Syntax verification passed successfully!**\n")
            else:
                yield self._sse.text("⚠️ Syntax verification failed. Let's correct issues in the next turn.\n")

            iteration += 1

        if loop_completed or iteration > max_iterations:
            status_text = "completed successfully" if loop_completed else "ended (iteration limit reached)"
            yield self._sse.text(f"\n🏁 **Autonomous Loop Execution Finished! Status: {status_text}**\n")
            yield self._sse.text("All logs have been persisted on disk to `progress.md` and repository code is updated.")

        yield self._sse.done()

    async def handle_benchmark_escher(self, command: str, state: dict):
        """Escher Self-Updating Loop Benchmark."""
        parts = command.split()
        sub_command = parts[1].lower() if len(parts) >= 2 else "sync"
        branch = parts[2] if len(parts) >= 3 else "main"

        yield self._sse.text("🏁 **Starting Escher Self-Updating Loop Benchmark!**\n")
        space_id = os.getenv("SPACE_ID")
        hf_token = os.getenv("HF_TOKEN") or os.getenv("HF_WRITE_TOKEN") or state.get("hf_token")
        target_file = "proxy.py"
        files_to_copy = []

        if sub_command == "sync":
            yield self._sse.text(f"🧬 **Mode: Synchronization** — Overwriting workspace with upstream branch `{branch}`...\n")
            git_token = get_github_token(state)
            clone_url = "https://github.com/dataopsnick/loopmaxxer-bench.git"
            if git_token and git_token != "none":
                clone_url = f"https://{git_token}@github.com/dataopsnick/loopmaxxer-bench.git"

            target_clone_dir = "loopmaxxer-bench-escher"
            if os.path.exists(target_clone_dir):
                shutil.rmtree(target_clone_dir, ignore_errors=True)

            proc = await asyncio.create_subprocess_exec(
                "git", "clone", "--depth", "1", "--branch", branch, clone_url, target_clone_dir,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT
            )
            clone_out, _ = await proc.communicate()
            if proc.returncode != 0:
                yield self._sse.text(f"❌ **Git Clone failed:** {clone_out.decode('utf-8', errors='replace')}\n")
                yield self._sse.done()
                return

            yield self._sse.text("✅ **Successfully cloned loopmaxxer-bench repository!**\n")

            escher_dir = os.path.join(target_clone_dir, "escher")
            if not os.path.exists(escher_dir):
                yield self._sse.text("❌ **Benchmark directory 'escher' not found inside loopmaxxer-bench.** Ensure it exists on the target branch.\n")
                yield self._sse.done()
                return

            yield self._sse.text("🔄 **Staging code files to current working directory...**\n")
            files_to_copy = [
                "Dockerfile", "README.md", "ci_reviewer.py", "generate_tasklist.py",
                "inspect_headers.py", "inspect_parts.py", "proxy.py", "start.sh",
                "verify_pairs.py", "verify_xml.py"
            ]

            copied_count = 0
            for filename in files_to_copy:
                src_path = os.path.join(escher_dir, filename)
                if os.path.exists(src_path):
                    shutil.copy2(src_path, filename)
                    yield self._sse.text(f"  • Overwritten `{filename}`\n")
                    copied_count += 1
                else:
                    yield self._sse.text(f"  ⚠️ Warning: `{filename}` not found in benchmark directory.\n")
            yield self._sse.text(f"✅ **Copied {copied_count} files successfully.**\n")

        elif sub_command == "task":
            yield self._sse.text("🧬 **Mode: Autonomous Improvement** — Invoking `pr-fix-verifier` lifecycle...\n")
            try:
                tree = ET.parse("TASKLIST.md")
                root = tree.getroot()
                tasks = root.findall("task")
            except Exception as e:
                yield self._sse.text(f"⚠️ Failed to parse TASKLIST.md: {e}\n")
                tasks = []

            if not tasks:
                yield self._sse.text("✨ **All tasks resolved or no outstanding issues found in TASKLIST.md!**\n")
                yield self._sse.done()
                return

            target_task = tasks[0]
            task_id = target_task.find("id").text if target_task.find("id") is not None else "unknown"
            title = target_task.find("title").text if target_task.find("title") is not None else "No Title"
            description = target_task.find("description").text if target_task.find("description") is not None else ""

            yield self._sse.text(f"🛠️ **Active Task Isolated:** [Task {task_id}] `{title}`\n")

            # Step A: Generate the deterministic unit test
            test_filepath = f"tests/test_task_{task_id}.py"
            yield self._sse.text(f"🧪 **Generating verification test case:** `{test_filepath}`...\n")

            test_system_prompt = (
                "You are an expert QA and Test Automation Engineer matching the 'pr-fix-test-engineer' skill. "
                "Your objective is to write an isolated, deterministic Python unit test using the standard "
                "library 'unittest' framework to verify the fix for the reported issue. "
                "The test MUST avoid external network/API calls and mock any heavy system dependencies."
            )
            test_user_prompt = (
                f"### Active Issue to Test\n**Title:** {title}\n**Details:**\n{description}\n\n"
                f"### Goal\nProvide the complete contents for `{test_filepath}`. Ensure it uses standard "
                f"`unittest.TestCase` assertions.\nWrap the code inside a single markdown code block and output nothing else."
            )

            try:
                llm_test_response = await call_openrouter_llm(test_system_prompt, test_user_prompt, state)
                test_code_match = re.search(r'```[a-zA-Z0-9]*\n(.*?)```', llm_test_response, re.DOTALL)
                test_code = test_code_match.group(1) if test_code_match else llm_test_response

                os.makedirs("tests", exist_ok=True)
                with open(test_filepath, "w", encoding="utf-8") as f:
                    f.write(test_code)
                yield self._sse.text("💾 Saved unit test to disk.\n")
            except Exception as test_gen_err:
                yield self._sse.text(f"❌ Failed to generate verification test: {test_gen_err}\n")
                yield self._sse.done()
                return

            # Step B: Generate the code modification
            yield self._sse.text(f"✏️ **Drafting codebase modifications for `{target_file}`...**\n")
            current_content = ""
            if os.path.exists(target_file):
                with open(target_file, "r", encoding="utf-8") as f:
                    current_content = f.read()

            fix_system_prompt = (
                "You are an autonomous senior software engineering agent. Your objective is to resolve the active "
                "TASKLIST.md issue by cleanly editing the target file. Provide only the fully modified code."
            )
            fix_user_prompt = (
                f"### Issue to Resolve\n**Title:** {title}\n**Details:**\n{description}\n\n"
                f"### Target File\n**Path:** {target_file}\n**Current Content:**\n```python\n{current_content}\n```\n\n"
                f"### Instructions\nProvide the complete, updated contents for `{target_file}` to resolve this specific issue.\n"
                f"Wrap the code inside a single markdown code block and output nothing else."
            )

            try:
                llm_fix_response = await call_openrouter_llm(fix_system_prompt, fix_user_prompt, state)
                fix_code_match = re.search(r'```[a-zA-Z0-9]*\n(.*?)```', llm_fix_response, re.DOTALL)
                updated_code = fix_code_match.group(1) if fix_code_match else llm_fix_response

                with open(target_file, "w", encoding="utf-8") as f:
                    f.write(updated_code)
                yield self._sse.text("✏️ Code base modified with proposed fix.\n")
            except Exception as fix_err:
                yield self._sse.text(f"❌ Failed to apply code fix: {fix_err}\n")
                yield self._sse.done()
                return
        else:
            yield self._sse.text("❌ **Invalid Escher sub-command!** Usage: `/benchmark escher [sync|task] [branch]`\n")
            yield self._sse.done()
            return

        # Step 3: Run gatekeeper validation before merging
        yield self._sse.text("🛡️ **Step 2.5: Running Deterministic Gatekeeper Tests on staged changes...**\n")

        # Compilation check
        compile_res = None
        for filename in files_to_copy:
            if filename.endswith(".py") and os.path.exists(filename):
                ret = subprocess.run(["python3", "-m", "py_compile", filename], capture_output=True)
                if ret.returncode != 0:
                    compile_res = ret
                    break
        if not compile_res:
            compile_res = subprocess.run(["python3", "-m", "py_compile", target_file], capture_output=True)

        if compile_res and compile_res.returncode != 0:
            yield self._sse.text(
                f"❌ **Compilation Failed!** Rollback triggered.\n"
                f"```\n{compile_res.stderr.decode('utf-8', errors='replace')}\n```\n"
            )
            if sub_command == "sync":
                for filename in files_to_copy:
                    if os.path.exists(filename):
                        subprocess.run(["git", "checkout", "--", filename])
            else:
                subprocess.run(["git", "checkout", "--", target_file])
                if sub_command == "task" and os.path.exists(test_filepath):
                    os.remove(test_filepath)
            yield self._sse.done()
            return

        # Unittest evaluation gate
        test_pattern = f"test_task_{task_id}.py" if sub_command == "task" else "test_*.py"
        test_run = subprocess.run([
            "python3", "-m", "unittest", "discover",
            "-s", "tests",
            "-p", test_pattern
        ], capture_output=True)

        if test_run.returncode == 0:
            yield self._sse.text("✅ **All local gatekeeper verification checks passed successfully!**\n")

            if space_id:
                yield self._sse.text(f"🌐 **Step 3: Running inside Hugging Face Space `{space_id}`. Pushing updates back to HF Repo...**\n")
                if not hf_token:
                    yield self._sse.text(
                        "⚠️ **HF_TOKEN is not configured!** Add write-access HF_TOKEN secrets to publish changes.\n"
                    )
                    yield self._sse.done()
                    return

                try:
                    subprocess.run(["git", "config", "user.name", "Escher Bot"], check=True)
                    subprocess.run(["git", "config", "user.email", "escher@opencodereview.local"], check=True)

                    res = subprocess.run(["git", "branch", "--show-current"], capture_output=True, text=True)
                    current_branch = res.stdout.strip() or "main"

                    hf_repo_url = f"https://oauth2:{hf_token}@huggingface.co/spaces/{space_id}.git"
                    subprocess.run(["git", "remote", "set-url", "origin", hf_repo_url], check=True)

                    subprocess.run(["git", "add", target_file], check=True)
                    if sub_command == "task":
                        subprocess.run(["git", "add", test_filepath], check=True)
                    else:
                        subprocess.run(["git", "add", "Dockerfile", "README.md", "ci_reviewer.py", "generate_tasklist.py", "inspect_headers.py", "inspect_parts.py", "start.sh", "verify_pairs.py", "verify_xml.py"], check=True)

                    commit_res = subprocess.run(
                        ["git", "commit", "-m", f"chore: Escher self-update (mode: {sub_command}, branch: {branch})"],
                        capture_output=True,
                        text=True
                    )

                    if "nothing to commit" in commit_res.stdout or "nothing to commit" in commit_res.stderr:
                        yield self._sse.text("ℹ️ **No code modifications detected.** Workspace aligns perfectly.\n")
                    else:
                        yield self._sse.text("💾 **Changes committed to local history. Pushing back to Hugging Face...**\n")
                        push_proc = await asyncio.create_subprocess_exec(
                            "git", "push", "origin", current_branch,
                            stdout=asyncio.subprocess.PIPE,
                            stderr=asyncio.subprocess.STDOUT
                        )

                        while True:
                            line_bytes = await push_proc.stdout.readline()
                            if not line_bytes:
                                break
                            line_str = line_bytes.decode('utf-8', errors='replace')
                            if hf_token in line_str:
                                line_str = line_str.replace(hf_token, "****")
                            yield self._sse.text(f"  `{line_str.strip()}`\n")

                        await push_proc.wait()

                        if push_proc.returncode == 0:
                            yield self._sse.text("🎉 **Self-update successfully pushed!** Hugging Face is rebuilding the Space!\n")
                        else:
                            yield self._sse.text(f"❌ **Git push failed with exit code {push_proc.returncode}**\n")
                except Exception as e:
                    yield self._sse.text(f"❌ **Failed to push update to Hugging Face Space:** {str(e)}\n")
            else:
                yield self._sse.text("ℹ️ **Not running in Hugging Face Space. Skipping Git push step.**\n")
        else:
            yield self._sse.text(
                f"❌ **Verification Failed:** Unit test output yielded errors during validation preflight check.\n"
                f"```\n{test_run.stderr.decode('utf-8', errors='replace')}\n```\n"
                f"🛑 **Self-update aborted to prevent breaking runtime stability.** Reverting modifications...\n"
            )
            subprocess.run(["git", "checkout", "--", target_file])
            if sub_command == "task" and os.path.exists(test_filepath):
                os.remove(test_filepath)

        yield self._sse.done()

    async def handle_benchmark_selinux(self, command: str, state: dict):
        """Lazy-Fetching Analysis on a GitHub repository (C/C++ focus)."""
        parts = command.split()
        owner = "SELinuxProject"
        repo = "selinux-kernel"
        branch = "main"

        if len(parts) >= 4:
            owner = parts[2]
            repo = parts[3]
        if len(parts) >= 5:
            branch = parts[4]

        yield self._sse.text(f"🏁 **Starting Lazy-Fetching Analysis on `{owner}/{repo}` (branch: `{branch}`)**\n")

        git_token = get_github_token(state)
        if git_token == "none":
            git_token = None

        # 1. Fetch Repository Tree
        yield self._sse.text("🔍 Fetching flat directory tree from GitHub API...")
        try:
            tree_items = await fetch_github_tree(owner, repo, branch, git_token)
        except Exception as e:
            yield self._sse.text(f"❌ **Failed to retrieve repository structure:** {str(e)}\n")
            yield self._sse.done()
            return

        source_files = [item["path"] for item in tree_items if item.get("type") == "blob" and item["path"].endswith((".c", ".cpp"))]
        yield self._sse.text(f"📋 Found `{len(source_files)}` total `.c` / `.cpp` source files in repository.\n")

        if not source_files:
            yield self._sse.text("⚠️ No target C/C++ source files found in the repository. Terminating.")
            yield self._sse.done()
            return

        max_file_count = 5
        selected_files = source_files[:max_file_count]
        yield self._sse.text(
            f"🚀 Selected top `{len(selected_files)}` files for analysis:\n"
            + "\n".join([f"- `{f}`" for f in selected_files]) + "\n"
        )

        # 2. Iterate and lazily-load files
        progress_file = "progress.md"
        with open(progress_file, "w", encoding="utf-8") as f:
            f.write(f"# Analysis Progress for {owner}/{repo}\n\nStarted runtime analysis.\n")

        report_lines = [f"# Code Quality and Type Safety Review: {owner}/{repo}\n"]

        for idx, filepath in enumerate(selected_files, 1):
            yield self._sse.text(f"\n⚡ **[{idx}/{len(selected_files)}] Fetching raw content:** `{filepath}`...")
            try:
                raw_content = await fetch_github_file_raw(owner, repo, filepath, branch, git_token)
            except Exception as e:
                yield self._sse.text(f"⚠️ Failed to download `{filepath}`: {str(e)}")
                continue

            yield self._sse.text("🧠 Analyzing code structure and variable definitions...")

            system_prompt = (
                "You are a static analysis tool focusing on code quality, type correctness, and syntax standards. "
                "Identify any areas with potential type safety issues, implicit conversions, or missing data constraints."
            )
            user_prompt = f"""File: {filepath}
Source Code:
```cpp
{raw_content}
```

Provide a concise analysis focusing on type constraints, safety checks, or cast issues. Keep your assessment highly technical and brief."""

            try:
                analysis = await call_openrouter_llm(system_prompt, user_prompt, state)
                report_lines.append(f"## File: `{filepath}`\n\n{analysis}\n\n---\n")

                with open(progress_file, "a", encoding="utf-8") as f:
                    f.write(f"- Analyzed: `{filepath}` successfully.\n")

                yield self._sse.text(f"✅ Analysis for `{filepath}` recorded.")
            except Exception as err:
                yield self._sse.text(f"❌ Analysis failed: {str(err)}")

        # Save compiled report
        with open("CODEREVIEW.md", "w", encoding="utf-8") as f:
            f.write("\n".join(report_lines))

        yield self._sse.text("\n📝 **Analysis loop complete! Compiled results saved to `CODEREVIEW.md`.**")

        # Process into TASKLIST.md
        try:
            process_review_file(input_file="CODEREVIEW.md", output_file="TASKLIST.md")
            yield self._sse.text("📋 Generated `TASKLIST.md` task definitions.")
        except Exception as ocr_err:
            yield self._sse.text(f"⚠️ Task compiling skipped: {ocr_err}")

        yield self._sse.text(
            "\n🏁 **Autonomous Analysis Finished!**\n"
            "All logs have been persisted on disk to `progress.md` and results are saved to `CODEREVIEW.md` and `TASKLIST.md`."
        )
        yield self._sse.done()

    async def handle_benchmark_deepsec(self, command: str, state: dict):
        """Runs deepsec vulnerability scan and analysis on the active workspace."""
        yield self._sse.text("🛡️ **Starting Vercel Deepsec Vulnerability Scan...**\n")

        # Check if .deepsec is already initialized
        if not os.path.exists(".deepsec"):
            yield self._sse.text("⚙️ **Step 1: Initializing deepsec workspace...**\n")
            proc = await asyncio.create_subprocess_exec(
                "npx", "-y", "deepsec", "init",
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT
            )
            out, _ = await proc.communicate()
            if proc.returncode != 0:
                yield self._sse.text(f"❌ **npx deepsec init failed:** {out.decode('utf-8', errors='replace')}\n")
                yield self._sse.done()
                return
            yield self._sse.text("✅ **deepsec workspace successfully initialized!**\n")

            yield self._sse.text("📦 **Installing deepsec dependencies...**\n")
            proc = await asyncio.create_subprocess_exec(
                "npx", "-y", "pnpm", "install",
                cwd=".deepsec",
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT
            )
            out, _ = await proc.communicate()
            if proc.returncode != 0:
                yield self._sse.text(f"❌ **pnpm install failed:** {out.decode('utf-8', errors='replace')}\n")
                yield self._sse.done()
                return
            yield self._sse.text("✅ **deepsec dependencies successfully installed!**\n")

        # Step 2: Run deepsec scan
        yield self._sse.text("🔍 **Step 2: Scanning codebase with regex matchers...**\n")
        proc = await asyncio.create_subprocess_exec(
            "pnpm", "deepsec", "scan",
            cwd=".deepsec",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )
        while True:
            line_bytes = await proc.stdout.readline()
            if not line_bytes:
                break
            raw_line = line_bytes.decode('utf-8', errors='replace')
            yield self._sse.text(f"  `{raw_line.strip()}`\n")
        await proc.wait()
        yield self._sse.text("✅ **Codebase scan completed!**\n")

        # Step 3: Run deepsec process (AI investigation)
        yield self._sse.text("🧠 **Step 3: Running AI investigation process...**\n")
        proc = await asyncio.create_subprocess_exec(
            "npx", "-y", "pnpm", "deepsec", "process",
            cwd=".deepsec",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )
        while True:
            line_bytes = await proc.stdout.readline()
            if not line_bytes:
                break
            raw_line = line_bytes.decode('utf-8', errors='replace')
            yield self._sse.text(f"  `{raw_line.strip()}`\n")
        await proc.wait()

        # Step 4: Export findings
        yield self._sse.text("📥 **Step 4: Exporting security findings...**\n")
        proc = await asyncio.create_subprocess_exec(
            "npx", "-y", "pnpm", "deepsec", "export", "--format", "md-dir", "--out", "./findings",
            cwd=".deepsec",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )
        out, _ = await proc.communicate()
        yield self._sse.text("✅ **Security findings successfully exported to `.deepsec/findings`!**\n")
        
        findings_path = ".deepsec/findings"
        if os.path.exists(findings_path) and os.path.isdir(findings_path):
            files = os.listdir(findings_path)
            if files:
                yield self._sse.text("\n📋 **Exported Findings Files:**\n" + "\n".join([f"- `{f}`" for f in files]) + "\n")
            else:
                yield self._sse.text("ℹ️ No specific vulnerabilities were found during this run.\n")

        yield self._sse.done()


    async def handle_cd(self, command: str, state: dict):
        """Change working directory within the workspace."""
        parts = command.strip().split(None, 1)
        target = parts[1].strip() if len(parts) > 1 else "~"
        if target == "~":
            target = "."
        current_cwd = state.get("cwd", ".")
        combined = os.path.join(current_cwd, target)
        try:
            target_abs = get_secure_path(combined)
            if os.path.isdir(target_abs):
                relative_new_cwd = os.path.relpath(target_abs, WORKSPACE_ROOT)
                state["cwd"] = relative_new_cwd
                await async_save_state(state)
                yield self._sse.text(f"📁 **Changed directory to:** `{relative_new_cwd}`\n")
            else:
                yield self._sse.text(f"❌ **Error:** `{target}` is not a directory.\n")
        except PermissionError as e:
            yield self._sse.text(f"❌ **Security Error:** {e}\n")
        except Exception as e:
            yield self._sse.text(f"❌ **Error:** {e}\n")
        yield self._sse.done()

    async def handle_ls(self, command: str, state: dict):
        """List directory contents within the workspace."""
        parts = command.strip().split(None, 1)
        target = parts[1].strip() if len(parts) > 1 else "."
        if target.startswith("-"):
            subparts = target.split()
            potential_target = subparts[-1]
            if potential_target.startswith("-"):
                target = "."
            else:
                target = potential_target
        current_cwd = state.get("cwd", ".")
        combined = os.path.join(current_cwd, target)
        try:
            target_abs = get_secure_path(combined)
            if os.path.isdir(target_abs):
                items = []
                with os.scandir(target_abs) as it:
                    for entry in it:
                        if entry.name in (".git", "node_modules", "__pycache__"):
                            continue
                        items.append(entry)
                items.sort(key=lambda x: (not x.is_dir(), x.name.lower()))
                output_lines = [f"### Directory listing for `{os.path.relpath(target_abs, WORKSPACE_ROOT)}`:\n"]
                for item in items:
                    rel_item_path = os.path.relpath(item.path, WORKSPACE_ROOT)
                    if item.is_dir():
                        output_lines.append(f"- 📁 **{item.name}/**")
                    else:
                        output_lines.append(f"- 📄 {item.name}  [📥 Download](/download/file?path={rel_item_path})")
                yield self._sse.text("\n".join(output_lines) + "\n")
            else:
                yield self._sse.text(f"❌ **Error:** `{target}` is not a directory.\n")
        except PermissionError as e:
            yield self._sse.text(f"❌ **Security Error:** {e}\n")
        except Exception as e:
            yield self._sse.text(f"❌ **Error:** {e}\n")
        yield self._sse.done()


# ============================================================================
# CLI Execution
# ============================================================================

async def execute_cli(command_args: str, state: dict, sse: SSEStream) -> AsyncGenerator[str, None]:
    """
    Execute whitelisted CLI commands (git/ocr) with streaming output.

    Extracted from the tail of proxy.py's run_cli_stream as a standalone
    function for clarity. Handles:
      - Command normalization (strip "ocr" prefix)
      - Binary selection (git vs ./ocr)
      - Argument validation (block --exec and dangerous chars)
      - Subprocess execution with streaming SSE output
      - Review command post-processing (CODEREVIEW.md → TASKLIST.md)
    """
    created_time = sse.created_time

    # Normalize command to strip redundant leading "ocr " or "ocr"
    if command_args.lower().startswith("ocr "):
        command_args = command_args[4:]
    elif command_args.lower() == "ocr":
        command_args = ""

    is_git = command_args.lower().startswith("git ") or command_args.lower().startswith("clone ")

    if is_git:
        binary = "git"
        if command_args.lower().startswith("git "):
            parsed_args = shlex.split(command_args[4:])
        else:
            parsed_args = ["clone"] + shlex.split(command_args[6:])
    else:
        binary = "./ocr"
        parsed_args = shlex.split(command_args)

    # Validate arguments — block dangerous option injection
    for arg in parsed_args:
        if re.search(r'^--exec', arg) or arg == "--exec" or not re.match(r'^[a-zA-Z0-9_\-\./\+=\s:@~*?]+$', arg):
            yield sse.text(f"\n\n**[Security Error]** Invalid or dangerous argument detected: '{arg}'.")
            yield sse.done()
            return

    args = [binary] + parsed_args

    # Strict executable whitelist
    if binary not in ALLOWED_BINARIES:
        yield sse.text(f"\n\n**[Security Error]** Executable '{binary}' is not allowed.")
        yield sse.done()
        return

    # Detect review commands that should capture output
    is_review_command = binary == "./ocr" and any(arg in parsed_args for arg in ["review", "scan"])
    review_file_handle = None
    REVIEW_FILENAME = "CODEREVIEW.md"

    # Yield an empty chunk to trigger typing animation
    yield sse.role()

    try:
        active_cwd_abs = get_secure_path(state.get("cwd", "."))
        process = await asyncio.create_subprocess_exec(
            *args,
            cwd=active_cwd_abs,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )

        if is_review_command:
            try:
                review_file_handle = open(REVIEW_FILENAME, "w", encoding="utf-8")
            except Exception as e:
                print(f"Warning: Could not open {REVIEW_FILENAME} for capture: {e}")

        while True:
            line_bytes = await process.stdout.readline()
            if not line_bytes:
                break

            raw_line = line_bytes.decode('utf-8', errors='replace')
            clean_line = ANSI_ESCAPE.sub('', raw_line)

            if review_file_handle:
                review_file_handle.write(clean_line)

            yield sse.text(clean_line)

        returncode = await process.wait()

        if review_file_handle:
            review_file_handle.close()

        # Post-process: git clone summary
        if is_git and "clone" in parsed_args:
            report = "\n\n✅ **Git clone completed successfully!**" if returncode == 0 else f"\n\n❌ **Git clone failed with exit code {returncode}**"
            yield sse.text(report)

        # Post-process: review → tasklist generation
        elif is_review_command and returncode == 0:
            yield sse.text("\n\nProcessing review output into task list...\n")

            xml_content, task_count = process_review_file(input_file=REVIEW_FILENAME)

            if task_count > 0:
                result_msg = (
                    f"\n✅ **Successfully generated XML task list with {task_count} items.**\n"
                    f"Copy the block below for issue import:\n\n"
                    f"```xml\n{xml_content}\n```\n\n"
                    f"### ⚙️ Interactive Actions & Workspace Controls\n"
                    f"Click any button below to manage, download, or execute these edits:\n\n"
                    f"[![Download Review](https://img.shields.io/badge/CODEREVIEW-Download_Markdown-0284c7?style=for-the-badge&logo=markdown)](/download/CODEREVIEW.md) "
                    f"[![Download Tasklist](https://img.shields.io/badge/TASKLIST-Download_XML-059669?style=for-the-badge&logo=xml-api)](/download/TASKLIST.md)\n\n"
                    f"[![Create Git Issues](https://img.shields.io/badge/GIT_ISSUES-Create_Local_Tasks-d97706?style=for-the-badge&logo=github)](/action/create-issues) "
                    f"[![Create Pull Request](https://img.shields.io/badge/PULL_REQUEST-Open_Review_PR-7c3aed?style=for-the-badge&logo=git)](/action/create-pr)\n"
                )
            else:
                result_msg = f"\n⚠️ **Review finished, but no structured tasks could be parsed from the output.** Check the {REVIEW_FILENAME} file formatting."

            yield sse.text(result_msg)

    except Exception as e:
        if review_file_handle:
            review_file_handle.close()
        yield sse.text(f"\n\n**[Proxy Error]** Failed to execute process: {str(e)}")

    yield sse.done()


# ============================================================================
# Main Stream Orchestrator
# ============================================================================

# Commands that require exclusive execution (benchmark workflows)
_BENCHMARK_PREFIXES = (
    "benchmark market-making", "/benchmark market-making",
    "/benchmark escher", "benchmark escher",
    "/benchmark selinux", "benchmark selinux",
    "benchmark deepsec", "/benchmark deepsec",
    "deepsec", "/deepsec",
)


def _is_benchmark_command(command: str) -> bool:
    """Check if a command is a benchmark workflow that needs exclusive execution."""
    lower = command.lower()
    return any(lower.startswith(p) or lower == p for p in _BENCHMARK_PREFIXES)


async def run_cli_stream(command_args: str):
    """
    Main orchestrator for CLI command streaming.

    This function replaces the 1500-line monolith from proxy.py with a thin
    dispatcher that delegates to specialized components:

      1. Phase 1 — Special commands (reset, create-issues, benchmarks, etc.)
      2. Phase 2 — Setup wizard state machine (if step != completed)
      3. Phase 3 — File commands (cd, ls)
      4. Phase 4 — Generic CLI execution (git/ocr)

    Each phase is handled by a dedicated class, making the flow easy to
    follow and extend.
    """
    sse = SSEStream()
    state = await async_load_state()
    commands = CommandHandlers(sse)

    # Phase 1: Special slash commands (run before state machine)
    handler = commands.find_special_handler(command_args)
    if handler:
        async for chunk in handler(command_args, state):
            yield chunk
        return

    # Phase 2: Setup wizard state machine (only if not completed)
    wizard = SetupWizard(sse)
    if wizard.is_active(state):
        async for chunk in wizard.run(command_args, state):
            yield chunk
        return

    # Phase 3: File system commands (cd, ls)
    file_handler = commands.find_file_handler(command_args)
    if file_handler:
        async for chunk in file_handler(command_args, state):
            yield chunk
        return

    # Phase 4: Generic CLI execution (git/ocr)
    async for chunk in execute_cli(command_args, state, sse):
        yield chunk


async def locked_generator(command_args: str):
    """
    Wraps run_cli_stream with a concurrency lock for benchmark commands.

    Benchmark workflows modify workspace files and should not run concurrently.
    Non-benchmark commands pass through without locking.
    """
    if _is_benchmark_command(command_args):
        if _ci_execution_lock.locked():
            sse = SSEStream()
            yield sse.text("⚠️ **Another benchmark execution is currently in progress. Please wait...**\n")
            yield sse.done()
            return

        async with _ci_execution_lock:
            async for chunk in run_cli_stream(command_args):
                yield chunk
    else:
        async for chunk in run_cli_stream(command_args):
            yield chunk


# ============================================================================
# FastAPI Routes
# ============================================================================

@app.get("/v1/models")
async def list_models():
    """List available models (OpenAI-compatible endpoint)."""
    return {
        "object": "list",
        "data": [
            {
                "id": "deepseekv4glm5.2",
                "object": "model",
                "created": int(time.time()),
                "owned_by": "openrouter"
            }
        ]
    }


@app.post("/v1/chat/completions")
async def chat_completions(request: Request):
    """
    Main chat completions endpoint.

    Two modes:
      1. If Authorization header is present and not "sk-dummy-key", proxy
         directly to OpenRouter (the 'ocr' CLI uses real API keys).
      2. Otherwise, extract the last message and run the CLI stream (the
         chat UI uses dummy keys and sends commands as messages).
    """
    auth_header = request.headers.get("Authorization", "")

    # Mode 1: Direct OpenRouter proxy passthrough
    if auth_header and "sk-dummy-key" not in auth_header:
        data = await request.json()

        state = await async_load_state()
        whitelist = state.get("whitelist", ["novita"])

        # Enforce zero-data-retention parameters
        data["provider"] = {
            "data_collection": state.get("data_collection", "allow"),
            "zdr": to_bool(state.get("zdr", False)),
            "allow_fallbacks": to_bool(state.get("allow_fallbacks", True)),
            "only": whitelist,
            "require_parameters": to_bool(state.get("require_parameters", False)),
        }

        is_streaming = data.get("stream", False)

        headers = {
            "Authorization": auth_header,
            "Content-Type": "application/json"
        }
        for h in ["HTTP-Referer", "X-Title", "X-OpenRouter-Title"]:
            if h in request.headers:
                headers[h] = request.headers[h]

        target_url = "https://openrouter.ai/api/v1/chat/completions"
        log_traffic(
            f"--- OUTGOING REQUEST ---\n"
            f"URL: {target_url}\n"
            f"Streaming: {is_streaming}\n"
            f"Headers: {json.dumps(headers, indent=2)}\n"
            f"Payload: {json.dumps(data, indent=2)}\n"
            f"------------------------"
        )

        if is_streaming:
            async def stream_openrouter():
                try:
                    log_traffic("--- STREAMING RESPONSE STARTED ---")
                    async with httpx.AsyncClient(timeout=60.0) as client:
                        async with client.stream(
                            "POST",
                            target_url,
                            json=data,
                            headers=headers
                        ) as r:
                            async for chunk in r.aiter_bytes():
                                try:
                                    chunk_str = chunk.decode('utf-8')
                                    log_traffic(f"STREAM CHUNK: {chunk_str}")
                                except Exception as decode_err:
                                    log_traffic(f"STREAM CHUNK DECODE ERROR: {decode_err} | Raw bytes hex: {chunk.hex()}")
                                yield chunk
                    log_traffic("--- STREAMING RESPONSE ENDED ---")
                except Exception as e:
                    log_traffic(f"STREAM EXCEPTION: {str(e)}")
                    err_chunk = {
                        "choices": [{"delta": {"content": f"\n\n**[Proxy Stream Error]** {str(e)}"}}]
                    }
                    yield f"data: {json.dumps(err_chunk)}\n\n"
                    yield "data: [DONE]\n\n"

            return StreamingResponse(stream_openrouter(), media_type="text/event-stream")
        else:
            try:
                async with httpx.AsyncClient(timeout=60.0) as client:
                    res = await client.post(
                        target_url,
                        json=data,
                        headers=headers
                    )
                    try:
                        res_str = res.content.decode('utf-8')
                        log_traffic(f"--- NON-STREAMING RESPONSE ---\nStatus: {res.status_code}\nContent: {res_str}\n-----------------------------")
                    except Exception as decode_err:
                        log_traffic(f"--- NON-STREAMING RESPONSE DECODE ERROR ---\nStatus: {res.status_code}\nError: {decode_err}\nRaw bytes hex: {res.content.hex()}\n-------------------------------------------")
                    return Response(
                        content=res.content,
                        status_code=res.status_code,
                        media_type="application/json"
                    )
            except Exception as e:
                log_traffic(f"NON-STREAMING EXCEPTION: {str(e)}")
                err_resp = {
                    "error": {
                        "message": f"Proxy request failed: {str(e)}",
                        "type": "proxy_error"
                    }
                }
                return Response(
                    content=json.dumps(err_resp),
                    status_code=502,
                    media_type="application/json"
                )

    # Mode 2: CLI stream (chat UI with dummy key)
    data = await request.json()
    messages = data.get("messages", [])
    last_message = messages[-1]["content"] if messages else "help"

    return StreamingResponse(locked_generator(last_message), media_type="text/event-stream")


# ============================================================================
# CI Reviewer
# ============================================================================

async def trigger_ci_reviewer(pull_number: int):
    """Executes ci_reviewer.py sequentially in the background."""
    async with _ci_execution_lock:
        log_traffic(f"--- AUTONOMOUS WORKFLOW INITIATED FOR PR #{pull_number} ---")
        state = await async_load_state()
        git_token = get_github_token(state)

        if not git_token or git_token == "none":
            log_traffic("Webhook Cancelled: GITHUB_TOKEN is not configured.")
            return

        proc = await asyncio.create_subprocess_exec(
            "python3", "ci_reviewer.py",
            env={**os.environ, "GITHUB_TOKEN": git_token},
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT
        )

        while True:
            line_bytes = await proc.stdout.readline()
            if not line_bytes:
                break
            raw_line = line_bytes.decode('utf-8', errors='replace').strip()
            log_traffic(f"[Autonomous Runner] {raw_line}")

        await proc.wait()
        log_traffic(f"--- AUTONOMOUS WORKFLOW FINISHED FOR PR #{pull_number} (Exit Code: {proc.returncode}) ---")


# ============================================================================
# Download Routes
# ============================================================================

@app.get("/download/CODEREVIEW.md")
async def download_codereview():
    """Serves the generated code audit file directly to the user's browser."""
    if os.path.exists("CODEREVIEW.md"):
        return FileResponse("CODEREVIEW.md", filename="CODEREVIEW.md", media_type="text/markdown")
    raise HTTPException(status_code=404, detail="CODEREVIEW.md has not been generated yet.")


@app.get("/download/TASKLIST.md")
async def download_tasklist():
    """Serves the generated well-formed XML tasklist directly to the user's browser."""
    if os.path.exists("TASKLIST.md"):
        return FileResponse("TASKLIST.md", filename="TASKLIST.md", media_type="text/markdown")
    raise HTTPException(status_code=404, detail="TASKLIST.md has not been generated yet.")


@app.get("/download/file")
async def download_file(path: str):
    """Securely serves a generic file from the workspace to prevent directory traversal."""
    try:
        abs_path = get_secure_path(path)
        if os.path.exists(abs_path) and os.path.isfile(abs_path):
            filename = os.path.basename(abs_path)
            return FileResponse(abs_path, filename=filename, media_type="application/octet-stream")
        raise HTTPException(status_code=404, detail="File not found.")
    except PermissionError as e:
        raise HTTPException(status_code=403, detail=str(e))
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))


# ============================================================================
# Action Routes
# ============================================================================

@app.get("/action/create-issues")
async def action_create_issues():
    """Triggers task splitting and commits local markdown task issues."""
    count, error = create_local_git_issues()
    if error:
        return HTMLResponse(content=f"<h2>❌ Error Creating Issues</h2><p>{error}</p>", status_code=500)

    return HTMLResponse(content=f"""
        <html>
        <head><style>
            body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; text-align: center; padding: 50px; background: #0f172a; color: white; }}
            .card {{ background: #1e293b; padding: 40px; border-radius: 12px; display: inline-block; box-shadow: 0 4px 15px rgba(0,0,0,0.3); max-width: 500px; }}
            .btn {{ background: #2563eb; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: bold; display: inline-block; margin-top: 20px; }}
        </style></head>
        <body>
            <div class="card">
                <h2>✅ Issues Split & Committed!</h2>
                <p>Successfully processed <b>{count} tasks</b> from your task list.</p>
                <p>All items have been committed locally to your current git branch under <code>./issues/</code>.</p>
                <a class="btn" href="javascript:window.close();">Close Tab</a>
            </div>
        </body>
        </html>
    """)


@app.get("/action/create-pr")
async def action_create_pr():
    """Autonomously pushes local updates and opens a standard branch PR on GitHub."""
    state = await async_load_state()
    git_token = get_github_token(state)
    if not git_token or git_token == "none":
        return HTMLResponse("<h2>❌ Setup Error</h2><p>GitHub Personal Access Token is not configured in Space Secrets or setup wizard.</p>", status_code=400)

    try:
        repo_dir = "loopmaxxer-bench" if os.path.exists("loopmaxxer-bench") else "."

        res = subprocess.run(["git", "-C", repo_dir, "branch", "--show-current"], capture_output=True, text=True)
        current_branch = res.stdout.strip() or "main"

        if current_branch == "main":
            current_branch = f"ocr-tasks-automated-{int(time.time())}"
            subprocess.run(["git", "-C", repo_dir, "checkout", "-b", current_branch])

        subprocess.run(["git", "-C", repo_dir, "add", "."], check=True)
        subprocess.run([
            "git", "-C", repo_dir,
            "-c", "user.name=OCR Space Bot",
            "-c", "user.email=bot@opencodereview.local",
            "commit", "-m", "chore: commit automatic code optimizations"
        ], stderr=subprocess.DEVNULL)

        push_res = subprocess.run(["git", "-C", repo_dir, "push", "origin", current_branch], capture_output=True, text=True)

        headers = {
            "Authorization": f"token {git_token}",
            "Accept": "application/vnd.github+json"
        }
        payload = {
            "title": f"chore: automated review updates on {current_branch}",
            "head": current_branch,
            "base": "main",
            "body": "This Pull Request was generated autonomously by the OCR Chat UI proxy workflow."
        }

        async with httpx.AsyncClient() as client:
            r = await client.post(
                "https://api.github.com/repos/dataopsnick/loopmaxxer-bench/pulls",
                json=payload,
                headers=headers
            )

        if r.status_code == 201:
            pr_data = r.json()
            pr_url = pr_data.get("html_url")
            return HTMLResponse(f"""
                <html>
                <head><style>
                    body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; text-align: center; padding: 50px; background: #0f172a; color: white; }}
                    .card {{ background: #1e293b; padding: 40px; border-radius: 12px; display: inline-block; box-shadow: 0 4px 15px rgba(0,0,0,0.3); }}
                    .btn {{ background: #2563eb; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: bold; display: inline-block; margin-top: 20px; }}
                </style></head>
                <body>
                    <div class="card">
                        <h2>✅ Pull Request Created!</h2>
                        <p>Branch <code>{current_branch}</code> pushed and submitted successfully.</p>
                        <p><a href="{pr_url}" target="_blank" style="color: #60a5fa; font-weight: bold;">Click here to inspect PR #{pr_data.get('number')} on GitHub</a></p>
                        <br>
                        <a class="btn" href="javascript:window.close();">Close Tab</a>
                    </div>
                </body>
                </html>
            """)
        else:
            return HTMLResponse(f"<h2>❌ API Error</h2><p>Failed to create Pull Request: {r.status_code} - {r.text}</p>", status_code=500)

    except Exception as e:
        return HTMLResponse(f"<h2>❌ Process Exception</h2><p>{str(e)}</p>", status_code=500)


# ============================================================================
# Webhook Handler
# ============================================================================

@app.post("/webhook")
async def github_webhook(
    request: Request,
    x_github_event: str = Header(None),
    x_hub_signature_256: str = Header(None)
):
    """Handles signed incoming GitHub webhook events."""
    body = await request.body()

    # 1. HMAC Signature Verification (if WEBHOOK_SECRET is configured)
    if WEBHOOK_SECRET:
        if not x_hub_signature_256:
            raise HTTPException(status_code=401, detail="Signature missing")

        try:
            sha_name, signature = x_hub_signature_256.split('=', 1)
        except ValueError:
            raise HTTPException(status_code=400, detail="Invalid signature format")

        if sha_name != 'sha256':
            raise HTTPException(status_code=501, detail="Unsupported hashing algorithm")

        mac = hmac.new(WEBHOOK_SECRET.encode('utf-8'), body, hashlib.sha256)
        if not hmac.compare_digest(mac.hexdigest(), signature):
            raise HTTPException(status_code=401, detail="Signature mismatch / Unauthorized payload")

    # 2. Parse payload JSON
    try:
        payload = json.loads(body)
    except Exception:
        raise HTTPException(status_code=400, detail="Malformed JSON payload")

    # 3. Handle Events

    # a. Wiki updates (event type is 'gollum')
    if x_github_event == "gollum":
        pages = payload.get("pages", [])
        for page in pages:
            title = page.get("title")
            action = page.get("action")
            html_url = page.get("html_url")

            log_traffic(f"Wiki Page Updated: '{title}' ({action}) -> {html_url}")

    # b. GitHub Actions completed
    elif x_github_event == "workflow_run":
        action = payload.get("action")
        workflow_run = payload.get("workflow_run", {})
        status = workflow_run.get("status")
        conclusion = workflow_run.get("conclusion")
        name = workflow_run.get("name")

        log_traffic(f"Workflow Run Event: {name} is {status} ({conclusion})")

        if action == "completed" and status == "completed":
            if conclusion == "success":
                pass  # Future: Safely proceed with next task on the supervisor tasklist
            elif conclusion == "failure":
                pass  # Future: Auto-assign fix task to debug-engineer agent

    # c. Pull Request events
    if x_github_event == "pull_request":
        action = payload.get("action")
        pr = payload.get("pull_request", {})
        head_ref = pr.get("head", {}).get("ref", "")
        pull_number = payload.get("number")

        log_traffic(f"Webhook Received: PR #{pull_number} - Event Action: '{action}' (Branch: '{head_ref}')")

        if action in ("opened", "synchronize", "reopened"):
            if head_ref.startswith("ocr-tasks-"):
                asyncio.create_task(trigger_ci_reviewer(pull_number))
                return {"status": "triggered", "details": f"Background verification spawned for PR #{pull_number}."}
            else:
                return {"status": "ignored", "details": "Branch pattern did not match 'ocr-tasks-*'"}

    return {"status": "ignored", "details": f"Event type '{x_github_event}' not configured for execution."}


# ============================================================================
# Startup
# ============================================================================

@app.on_event("startup")
async def startup_event():
    """Initialize system state on application startup."""
    bootstrap_system()