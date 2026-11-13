```markdown
---
name: dsh-superagent
description: Operate, configure, and interface with dsh-superagent via CLI, Management HTTP API, or as an agent tool-call service over JSON-RPC 2.0, TCP/IP, Unix socket daemon, file-buffer spool, and MCP stdio.
---

# dsh-superagent CLI & Agent Tool Interface

`dsh-superagent` is a proactive coding-agent bundle for DeepSeek Harness (`dsh`). It watches Sources (GitHub, Feishu, or a Backend RPC source), turns authorized requests into `Ticket`s, runs each in an isolated DSH Executor Session + directory, and delivers results back to the originating channel.

---

## 1. Service CLI (`scripts/dsh-superagent`)

Manage the dedicated DSH profile (`superagent` by default) running inside tmux session `dsh-<profile>`:

```sh
scripts/dsh-superagent start
scripts/dsh-superagent status
scripts/dsh-superagent restart
scripts/dsh-superagent stop
```

### Flags & Environment Variables

| Flag | Default / Env Var | Purpose |
| --- | --- | --- |
| `--profile <name>` | `DSH_SUPERAGENT_PROFILE` (`superagent`) | Target DSH profile (tmux session `dsh-<profile>`) |
| `--url <url>` | `DSH_SUPERAGENT_URL` (`http://127.0.0.1:3082`) | Management API base URL |
| `--timeout <sec>` | `60` | Startup health-check timeout |
| `--stop-timeout <sec>` | `300` | Graceful `SIGINT` shutdown wait (never force-kills) |
| `--respect-stop` | off | For cron/hourly checks: skip `start`/`restart` if intentionally stopped |

### Exit Codes

| Code | Meaning | What to do |
| --- | --- | --- |
| `0` | Healthy (`status` JSON printed to stdout) | Ready |
| `1` | Stopped, duplicated, unreachable, runtime error, or timeout | `restart` usually recovers; inspect newest `.artifacts/dsh-superagent-<profile>-*.log` |
| `2` | Running, but a non-runtime part (`errors` in JSON) is failing | Do not restart; fix the named Source/component |
| `3` | Running, waiting on setup (`setup` in JSON) | Do not restart; complete GitHub/CLI/environment setup |

Logs and control files live under `.artifacts/` (`dsh-superagent-<profile>-*.log`, keeping newest 20, and `.artifacts/control/profiles/<profile>/`).

---

## 2. Installation, Profile & Wrappers

From a repository checkout (Node 22.19+/24+, `pnpm`, `git` 2.32+, `tmux`, Python 3.8+, `gh` 2.48+, `dsh`):

```sh
pnpm install && pnpm build
pnpm check   # typecheck + vitest + build
```

Check for an existing profile before creating one (never run `--from-default-profile` over an existing profile—it deletes `cordis.patch.yml`):

```sh
grep -l -E 'id: dsh-superagent\b' "${DSH_HOME:-$HOME/.dsh}"/profiles/*/cordis.patch.yml 2>/dev/null

# Fresh install only:
dsh superagent --from-default-profile web --dump-config > /dev/null
dsh plugin --profile superagent add "link:$PWD"
```

Edit `${DSH_HOME:-$HOME/.dsh}/profiles/superagent/cordis.patch.yml` (see `examples/profile.cordis.patch.yml`):

```yaml
- id: webserver
  config:
    host: 127.0.0.1
    port: 3082
- id: dsh-superagent
  config:
    stateDir: /abs/path/to/.dsh/state/superagent
# Optional: enable Feishu Source
# - id: dsh-superagent-feishu
#   disabled: false
```

### GitHub & Optional Feishu CLI Wrappers

Setup probes CLIs with `GH_TOKEN`/`GITHUB_TOKEN` stripped. Use dedicated wrappers:

```sh
# Required for GitHub Source (must be a user account with write/push access, not a GitHub App)
mkdir -p ~/.local/bin
cat > ~/.local/bin/gh-agent <<'EOF'
#!/bin/sh
GH_CONFIG_DIR="$HOME/.config/gh-agent" exec gh "$@"
EOF
chmod +x ~/.local/bin/gh-agent
gh-agent auth login
gh-agent api user --jq .login

# Optional Feishu bot (must use strict-mode bot)
LARKSUITE_CLI_CONFIG_DIR="$HOME/.lark-cli-dsh-superagent" lark-cli config init --app-id <cli_...> --app-secret-stdin
LARKSUITE_CLI_CONFIG_DIR="$HOME/.lark-cli-dsh-superagent" lark-cli config strict-mode bot --global
cat > ~/.local/bin/lark-bot <<'EOF'
#!/bin/sh
export LARKSUITE_CLI_CONFIG_DIR="$HOME/.lark-cli-dsh-superagent"
exec lark-cli "$@"
EOF
chmod +x ~/.local/bin/lark-bot
```

---

## 3. Management HTTP API (`curl`)

Base URL: `http://127.0.0.1:3082/orchestrator/api/` with header `x-orchestrator-request: 1`.

```sh
API="http://127.0.0.1:3082/orchestrator/api"
H="x-orchestrator-request: 1"
```

### Core Endpoints

| Route | Method / Body | Purpose |
| --- | --- | --- |
| `state` | `GET` | `setup`, `health`, `load` (`running`, `queued`, `maxWorkers`), `paused`, `masterId` |
| `overview` | `GET` | Pending `person` Tickets, active `problems`, 24h `flow` counts |
| `settings` | `GET` | `blocks` (`policy`, `eval`, `executor` + `revision`), `sources`, `environment` |
| `sources` | `GET` | Per-source `state`, `health`, 24h `stats`, `filtered` triggers, `dropped` replies |
| `settings/<block>` | `PUT` `{"value": ..., "revision": n}` | Save `policy`, `eval`, `executor`, `source:github`, or `source:feishu` |
| `executor/pause` | `POST` `{"paused": true\|false}` | Pause or resume Eval, dispatch, and outbound replies |
| `setup/check` | `POST` | Re-run setup checks (restarts service) |
| `policy-sync/<id>` | `POST` | Sync allowed list from external chat (e.g. `feishu`) |
| `prompts` | `GET` / `PUT` `{"config", "baseRevision"}` | Read/save `communication`, `scope`, `delivery`, `skills`, `overrides` |
| `tickets` / `tickets/:id` | `GET` | List/inspect Tickets (`?state=open\|person\|working\|idle\|ended`) |
| `tickets/<id>/retry` | `POST` | Retry a `person/delivery`, `person/blocked`, or `person/failed` Ticket |
| `tickets/<id>/stop` | `POST` | Stop a Ticket and withdraw its Mandate |
| `threads` / `threads/:key` | `GET` | List Threads / inspect timeline (`?filter=subscribed\|evaluating\|recent\|all`) |
| `search?q=...` | `GET` | Search Tickets, Threads, and Triggers (`#12`, text, URLs, `om_...`) |

**Note on `PUT settings/<block>`:** When a save returns `"restarting": true` (e.g. changing `cli` or `repository`), wait until `scripts/dsh-superagent status` stops exiting `1` and re-read revisions from `GET settings` before the next write.

### Quick Setup via `curl`

```sh
# 1. GitHub Source
rev=$(curl -sf -H "$H" "$API/settings" | jq '.sources[] | select(.id == "github") | .revision')
curl -sf -X PUT -H "$H" -H 'content-type: application/json' "$API/settings/source:github" \
  -d "{\"value\":{\"enabled\":true,\"config\":{\"repository\":\"owner/repo\",\"cli\":\"$HOME/.local/bin/gh-agent\"}},\"revision\":$rev}"

# 2. Policy allowed senders (requests from unlisted accounts are ignored by default)
rev=$(curl -sf -H "$H" "$API/settings" | jq '.blocks.policy.revision')
curl -sf -X PUT -H "$H" -H 'content-type: application/json' "$API/settings/policy" \
  -d "{\"value\":{\"sources\":{\"github\":{\"allowed\":[{\"id\":\"your-github-login\"}]}}},\"revision\":$rev}"

# 3. Executor worktree prepare command
rev=$(curl -sf -H "$H" "$API/settings" | jq '.blocks.executor.revision')
curl -sf -X PUT -H "$H" -H 'content-type: application/json' "$API/settings/executor" \
  -d "{\"value\":{\"prepare\":\"git -C $HOME/src/repo fetch --no-tags \\\"https://github.com/\$DSH_SUPERAGENT_GITHUB_REPOSITORY.git\\\" \\\"\${DSH_SUPERAGENT_GITHUB_PR_HEAD:-HEAD}:refs/dsh-superagent/\$DSH_SUPERAGENT_TICKET\\\" && git -C $HOME/src/repo worktree add -b \\\"agent/\$DSH_SUPERAGENT_NAME\\\" \\\"\$DSH_SUPERAGENT_DIR\\\" \\\"refs/dsh-superagent/\$DSH_SUPERAGENT_TICKET\\\"\"},\"revision\":$rev}"
```

Trigger a test run from GitHub CLI (post a comment; editing an existing issue body does not fire a notification):

```sh
gh issue comment 1 --repo owner/repo --body "@<agent-login> Fix the typo in README.md"
```

---

## 4. Backend / Agent Tool-Call Interface (RPC, TCP/IP, Daemon, File-Buffer, MCP)

To let a backend server or external agent invoke `dsh-superagent` directly as tool calls, use the **JSON-RPC 2.0 / MCP Gateway** (`RpcGateway` + `backendKind` source).

### Available Agent Tools

All transports speak identical **JSON-RPC 2.0** (`tools/list`, `tools/call`, `initialize`, or direct shorthand methods `superagent.submit`, `superagent.status`, `superagent.read`, `superagent.wait`, `superagent.reply_poll`, `superagent.control`):

| Tool | Key Arguments | Behavior |
| --- | --- | --- |
| `superagent_submit` | `thread_id`, `text`, `actor_id?` (`"rpc"`), `request_id?`, `auto_accept?`, `title?`, `worktree_name?`, `wait?`, `timeout_ms?` | Injects a `Trigger` on `backend:<thread_id>`. If `auto_accept: true`, creates a Ticket immediately (auto-deriving `title` and `worktree_name` if omitted). If `wait: true`, blocks until Ticket reaches `person/*` or `ended` and returns report + replies. |
| `superagent_status` | `ticket_id?` | Returns global `health`/`load`/`paused`, or a Ticket's `stage`, `latest_report`, and buffered `replies`. |
| `superagent_read` | `resource`, `offset?`, `limit?` | Reads any `orchestrator_read` resource (`tickets`, `ticket/<id>`, `ticket/<id>/result`, `threads`, `thread/<key>`, `triggers`, `decisions`, `settings`, `prompts`). |
| `superagent_wait` | `ticket_id`, `timeout_ms?`, `after_seq?` | Long-polls until Ticket reaches `person/*` or `ended`, returning status + new `replies`. |
| `superagent_reply_poll` | `ticket_id?`, `thread_id?`, `after_seq?` | Fetches buffered acknowledgements, replies, and notices sent to the `backend` channel. |
| `superagent_control` | `action` (`stop`\|`retry`\|`note`\|`request_status`\|`pause`\|`resume`), `ticket_id?`, `text?` | Stops/retries a Ticket, injects a context note, requests waiting status, or pauses/resumes runtime. |

### Transports & Environment Variables

Set any combination of these env vars when starting `dsh-superagent`:

| Transport | Env Var | Example |
| --- | --- | --- |
| **HTTP JSON-RPC 2.0** | (always on when `RpcGateway` is wired) | `POST http://127.0.0.1:3082/orchestrator/api/rpc` |
| **Bearer Token Auth** | `DSH_SUPERAGENT_RPC_TOKEN` | `Authorization: Bearer <token>` (alternative to `x-orchestrator-request: 1`) |
| **TCP/IP Daemon** | `DSH_SUPERAGENT_RPC_TCP` | `127.0.0.1:3083` (NDJSON; set `DSH_SUPERAGENT_RPC_ALLOW_REMOTE=1` to bind non-loopback) |
| **Unix Socket Daemon** | `DSH_SUPERAGENT_RPC_UNIX` | `/tmp/dsh-superagent-rpc.sock` (NDJSON + live push notifications) |
| **File-Buffer Spool** | `DSH_SUPERAGENT_RPC_SPOOL` | `/path/to/rpc-buffer` (`in/*.json` → `out/*.json` + `events/*.json`) |
| **Open Backend Policy** | `DSH_SUPERAGENT_RPC_OPEN` | `1` to accept any `actor_id` on `backend` without adding to `policy.sources.backend.allowed` |

### Calling Examples by Transport

**1. HTTP JSON-RPC 2.0 (`curl`):**
```sh
curl -sf -X POST http://127.0.0.1:3082/orchestrator/api/rpc \
  -H 'x-orchestrator-request: 1' \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"superagent_submit","arguments":{"thread_id":"job-42","text":"Fix the cache bug in src/app/cache.ts","auto_accept":true,"wait":true}}}'
```

**2. TCP/IP Daemon (NDJSON + streaming `notifications/superagent/event` and `notifications/superagent/reply`):**
```sh
printf '{"jsonrpc":"2.0","id":1,"method":"tools/list"}\n' | nc 127.0.0.1 3083
```

**3. Unix Socket Daemon:**
```sh
printf '{"jsonrpc":"2.0","id":1,"method":"superagent.status","params":{}}\n' | nc -U /tmp/dsh-superagent-rpc.sock
```

**4. File-Buffer Spool (atomic `.tmp` → `.json` rename; no network required):**
```sh
SPOOL=/tmp/dsh-rpc-buffer
cat > "$SPOOL/in/.req1.tmp" <<'EOF'
{"jsonrpc":"2.0","id":"req1","method":"superagent.submit","params":{"thread_id":"job-43","text":"Check failing tests","auto_accept":true}}
EOF
mv "$SPOOL/in/.req1.tmp" "$SPOOL/in/req1.json"
# Read response from $SPOOL/out/req1.json; async events appear in $SPOOL/events/*.json
```

**5. Stdio MCP Server (`scripts/dsh-superagent-rpc`):**
```json
{
  "mcpServers": {
    "dsh-superagent": {
      "command": "python3",
      "args": ["/path/to/dsh-superagent-prototype/scripts/dsh-superagent-rpc", "--tcp", "127.0.0.1:3083"]
    }
  }
}
```
(Without `--tcp`/`--unix`/`--spool`, `scripts/dsh-superagent-rpc` forwards to `http://127.0.0.1:3082/orchestrator/api/rpc`.)

---

## 5. Adding the RPC Gateway to the Codebase (If Not Present)

If the target checkout does not yet have `src/app/rpc.ts`, `src/sources/backend.ts`, and `scripts/dsh-superagent-rpc`, apply the minimal integration below (no `cordis.patch.yml` or build-config changes needed—`RpcGateway` auto-registers `backendKind` in `Runtime` and relaxes `title`/`worktree_name`/caller defaults):

1. **`src/kv.ts`** — add under `KV`:
   ```ts
   backend: {
     seq: record<number>('backend-seq'),
     replies: record<{ seq: number; id: string; kind: 'acknowledgement' | 'reply' | 'notice'; ticketId?: string; thread: string; text: string; at: string }[]>('backend-replies'),
   },
   ```
2. **`src/sources/backend.ts`** — `SourceKind` (`kind: 'backend'`, `health: ['backend']`) with `backendTrigger(...)` and `setBackendSink(fn)` for `channel.send`.
3. **`src/app/rpc.ts`** — `RpcGateway` class implementing `tools/list`, `tools/call`, `superagent.*`, HTTP `handle()`, `net.createServer` NDJSON TCP/Unix daemon, and `fs.watch` + 250ms poll file-buffer spooler. Auto-slugifies `worktree_name` (`rpc-${key(threadId).slice(0, 8)}`) and derives `title` from `text` when `auto_accept: true` omits them; skips `not_a_member` rejection when `DSH_SUPERAGENT_RPC_OPEN=1` (by marking `acceptUnlisted` / setting `unlistedAddressed: 'explicit'` in `backendKind`).
4. **`src/app/http.ts`** — add optional `rpc?(payload: unknown): Promise<unknown>` to `HttpHost` and handle `POST /orchestrator/api/rpc` in `registerHttp`.
5. **`src/app/runtime.ts`** — instantiate `this.rpc = new RpcGateway(...)`, call `this.disposers.push(...this.rpc.start())` in `initialize()`, forward `emit: event => { this.rpc?.emit(event); ... }`, and expose `rpc: payload => this.rpc.handle(payload)` on `httpHost()`.
6. **`scripts/dsh-superagent-rpc`** — Python stdio-to-HTTP/TCP/Unix/Spool bridge for MCP clients.

---

## 6. Upgrades & Troubleshooting

- **Safe upgrade:** Wait until `load.running == 0` in `GET state`, back up `<stateDir>/orchestrator.sqlite` (schema migrations are one-way), run `git pull && pnpm install && pnpm build`, then `scripts/dsh-superagent restart && scripts/dsh-superagent status`.
- **No response on `:3082`:** Service is restarting or failed startup config; check `.artifacts/dsh-superagent-*.log`.
- **Nothing happens after a request:** Check `GET state` (`paused` must be `false`, `setup.problem` must be `null`), `GET settings` (Source `enabled: true` and caller in `allowed` or `DSH_SUPERAGENT_RPC_OPEN=1`), and `GET sources` (`filtered` / `dropped`).
```
