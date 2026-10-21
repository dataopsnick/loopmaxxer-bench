#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# Configuration (Override via Environment Variables or CLI flags)
# Usage:
#   ./test_headless.sh [BASE_URL] [COMMAND] [MODEL]
# Examples:
#   ./test_headless.sh http://127.0.0.1:7860
#   ./test_headless.sh https://dataopsnick-escher.hf.space "/tunnel" "google/gemini-4-argon"
# ==============================================================================
BASE_URL="${1:-${ESCHER_URL:-http://127.0.0.1:7860}}"
BASE_URL="${BASE_URL%/}" # Strip trailing slash
TEST_CMD="${2:-${HEADLESS_COMMAND:-ls}}"
TEST_MODEL="${3:-${PREFERRED_MODEL:-project-blue-morpho}}"
HF_TOKEN_HEADER=()
if [ -n "${HF_TOKEN:-}" ]; then
  HF_TOKEN_HEADER=(-H "Authorization: Bearer $HF_TOKEN")
fi

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info()  { echo -e "${BLUE}ℹ️  $1${NC}"; }
log_ok()    { echo -e "${GREEN}✅ $1${NC}"; }
log_warn()  { echo -e "${YELLOW}⚠️  $1${NC}"; }
log_err()   { echo -e "${RED}❌ $1${NC}"; }

# Helper to parse OpenAI SSE stream into clean text
stream_chat_command() {
  local url="$1"
  local model="$2"
  local cmd="$3"

  curl -sN -X POST "$url/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer sk-dummy-key" \
    "${HF_TOKEN_HEADER[@]}" \
    -d "{\"model\": \"$model\", \"stream\": true, \"messages\": [{\"role\": \"user\", \"content\": \"$cmd\"}]}" \
  | while IFS= read -r line; do
      if [[ "$line" == "data: [DONE]"* ]]; then
        break
      elif [[ "$line" == data:* ]]; then
        json_part="${line#data: }"
        # Extract choices[0].delta.content using python3 (always available) or jq
        python3 -c 'import sys, json; 
try:
    d = json.loads(sys.argv[1])
    c = d.get("choices", [{}])[0].get("delta", {}).get("content", "")
    sys.stdout.write(c)
except Exception:
    pass' "$json_part"
      fi
    done
  echo ""
}

echo "=================================================================="
echo "🦋 Project Blue Morpho — Headless Endpoint Verification Suite"
echo "   Target:  $BASE_URL"
echo "   Model:   $TEST_MODEL"
echo "   Command: $TEST_CMD"
echo "=================================================================="

# ------------------------------------------------------------------------------
# 1. Wake-from-Sleep / Readiness Preflight Check
# ------------------------------------------------------------------------------
log_info "[1/5] Checking endpoint readiness (waking Space if sleeping)..."
READY=false
for i in {1..20}; do
  HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${HF_TOKEN_HEADER[@]}" "$BASE_URL/v1/models" || echo "000")
  if [ "$HTTP_CODE" = "200" ]; then
    READY=true
    break
  fi
  log_warn "Attempt $i/20: Endpoint returned HTTP $HTTP_CODE (waking up or starting)... waiting 3s"
  sleep 3
done

if [ "$READY" != "true" ]; then
  log_err "Endpoint $BASE_URL/v1/models did not become ready in time."
  exit 1
fi
log_ok "Endpoint is awake and responding (HTTP 200)!"

# ------------------------------------------------------------------------------
# 2. Verify GET /v1/models (Project Blue Morpho + Latest Models)
# ------------------------------------------------------------------------------
log_info "[2/5] Testing GET /v1/models..."
MODELS_JSON=$(curl -s "${HF_TOKEN_HEADER[@]}" "$BASE_URL/v1/models")

if echo "$MODELS_JSON" | grep -q "project-blue-morpho" && echo "$MODELS_JSON" | grep -q "google/gemini-4-argon"; then
  MODEL_COUNT=$(python3 -c 'import sys, json; print(len(json.loads(sys.argv[1]).get("data", [])))' "$MODELS_JSON")
  log_ok "Found $MODEL_COUNT models including 'project-blue-morpho' and 'google/gemini-4-argon'."
else
  log_err "Expected models missing from /v1/models response:"
  echo "$MODELS_JSON"
  exit 1
fi

# ------------------------------------------------------------------------------
# 3. Verify GET /v1/tunnel (Cloudflared Tunnel & Webhook Status)
# ------------------------------------------------------------------------------
log_info "[3/5] Testing GET /v1/tunnel..."
TUNNEL_JSON=$(curl -s "${HF_TOKEN_HEADER[@]}" "$BASE_URL/v1/tunnel")
echo "   Response: $TUNNEL_JSON"

TUNNEL_MODE=$(python3 -c 'import sys, json; print(json.loads(sys.argv[1]).get("mode", "unknown"))' "$TUNNEL_JSON")
TUNNEL_URL=$(python3 -c 'import sys, json; print(json.loads(sys.argv[1]).get("tunnel_url") or "")' "$TUNNEL_JSON")
log_ok "Tunnel endpoint verified (mode: $TUNNEL_MODE, tunnel_url: ${TUNNEL_URL:-none})"

# Optional: If a live cloudflared tunnel URL was returned, verify it responds too!
if [ -n "$TUNNEL_URL" ] && [[ "$TUNNEL_URL" == https://* ]]; then
  log_info "   ↳ Probing discovered Cloudflared Tunnel URL: $TUNNEL_URL/v1/models"
  CF_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$TUNNEL_URL/v1/models" || echo "000")
  if [ "$CF_CODE" = "200" ]; then
    log_ok "   ↳ Cloudflared Tunnel ($TUNNEL_URL) is publicly reachable (HTTP 200)!"
  else
    log_warn "   ↳ Cloudflared Tunnel ($TUNNEL_URL) returned HTTP $CF_CODE (may still be propagating)."
  fi
fi

# ------------------------------------------------------------------------------
# 4. Verify POST /v1/chat/completions with '/tunnel' Slash Command
# ------------------------------------------------------------------------------
log_info "[4/5] Testing '/tunnel' slash command over SSE stream..."
TUNNEL_CMD_OUT=$(stream_chat_command "$BASE_URL" "project-blue-morpho" "/tunnel")
echo "------------------------------------------------------------------"
echo "$TUNNEL_CMD_OUT"
echo "------------------------------------------------------------------"
if echo "$TUNNEL_CMD_OUT" | grep -q "Tunnel Mode"; then
  log_ok "'/tunnel' SSE command succeeded!"
else
  log_err "'/tunnel' SSE command did not return expected output."
  exit 1
fi

# ------------------------------------------------------------------------------
# 5. Verify Headless Command Execution & Dynamic Model Selection
# ------------------------------------------------------------------------------
log_info "[5/5] Executing headless command '$TEST_CMD' with model '$TEST_MODEL'..."
CMD_OUT=$(stream_chat_command "$BASE_URL" "$TEST_MODEL" "$TEST_CMD")
echo "------------------------------------------------------------------"
echo "$CMD_OUT"
echo "------------------------------------------------------------------"

if echo "$CMD_OUT" | grep -q "❌"; then
  log_err "Headless command '$TEST_CMD' reported an error marker (❌)."
  exit 1
fi

log_ok "All Project Blue Morpho headless checks passed!"