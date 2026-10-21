#!/bin/bash
set -e

# 1. Add POSIX signal trap to reap background daemons
cleanup() {
  echo "Reaping background daemons..."
  kill $(jobs -p) 2>/dev/null || true
}
trap cleanup EXIT

# Start and verify MongoDB
echo "Starting local MongoDB..."
mkdir -p $HOME/data/db
mongod --dbpath $HOME/data/db --bind_ip 127.0.0.1 --port 27017 > /dev/null 2>&1 &

for i in {1..10}; do
  if nc -z 127.0.0.1 27017; then
    echo "MongoDB successfully started."
    break
  fi
  echo "Waiting for MongoDB to initialize..."
  sleep 1
done

# 2. Ensure Python Proxy dependencies are fully installed
echo "Ensuring Python Proxy dependencies are installed..."
$HOME/venv/bin/pip install -q fastapi uvicorn httpx gitingest

# 2.5 Initialize and install deepsec
echo "Initializing and installing deepsec in the app workspace..."
cd $HOME/app
if [ ! -d ".deepsec" ]; then
  npx -y deepsec init --scaffold-only || echo "Deepsec scaffold skipped."
fi
if [ -d ".deepsec" ]; then
  (cd .deepsec && npx -y pnpm install) || echo "Deepsec dependency install skipped."
fi
cd $HOME/app

# 2.8 Start Cloudflared Tunnel (Named DDNS or Ephemeral Quick Tunnel over HTTP/2)
if command -v cloudflared >/dev/null 2>&1; then
  if [ -n "$CF_TUNNEL_TOKEN" ]; then
    echo "🌐 Starting Persistent Cloudflare Named Tunnel (HTTP/2)..."
    cloudflared tunnel --no-autoupdate --protocol http2 run --token "$CF_TUNNEL_TOKEN" --url http://127.0.0.1:7860 > /tmp/cloudflared.log 2>&1 &
    if [ -n "$CF_PUBLIC_HOSTNAME" ]; then
      export PUBLIC_INGRESS_URL="https://${CF_PUBLIC_HOSTNAME#https://}"
      echo "$PUBLIC_INGRESS_URL" > $HOME/app/PUBLIC_URL.txt
      echo "✅ Persistent Tunnel Hostname: $PUBLIC_INGRESS_URL"
    fi
  elif [ "${ENABLE_QUICK_TUNNEL:-true}" = "true" ]; then
    echo "🌐 Starting Ephemeral Cloudflare Quick Tunnel (HTTP/2)..."
    cloudflared tunnel --no-autoupdate --protocol http2 --url http://127.0.0.1:7860 > /tmp/cloudflared.log 2>&1 &
    for i in {1..15}; do
      TUNNEL_URL=$(grep -oE 'https://[a-zA-Z0-9-]+\.trycloudflare\.com' /tmp/cloudflared.log | head -n 1 || true)
      if [ -n "$TUNNEL_URL" ]; then
        export PUBLIC_INGRESS_URL="$TUNNEL_URL"
        echo "$PUBLIC_INGRESS_URL" > $HOME/app/PUBLIC_URL.txt
        echo "✅ Ephemeral Quick Tunnel Active: $PUBLIC_INGRESS_URL"
        break
      fi
      echo "Waiting for Quick Tunnel URL..."
      sleep 1
    done
  fi
fi

# 3. Start and verify Python Proxy
echo "Starting Local CLI-to-OpenAI Proxy..."
$HOME/venv/bin/python3 -m uvicorn proxy:app --host 127.0.0.1 --port 8080 &
 
for i in {1..15}; do
  if curl -s http://127.0.0.1:8080/v1/models > /dev/null; then
    echo "Proxy process started successfully."
    break
  fi
  echo "Waiting for Proxy..."
  sleep 1
done

# 4. Configure Chat-UI
echo "Configuring Chat-UI..."
cd $HOME/chat-ui

cat << 'EOF' > .env.local
MONGODB_URL=mongodb://127.0.0.1:27017/chatui
OPENAI_BASE_URL=http://127.0.0.1:8080/v1
OPENAI_API_KEY=sk-dummy-key
MODELS='[
  {
    "name": "Project Blue Morpho",
    "id": "project-blue-morpho",
    "description": "Escher Autonomous Loop Router (uses active wizard model or select a specific model below).",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "google/gemini-4-argon",
    "id": "google/gemini-4-argon",
    "description": "Google Gemini 4 Argon",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~google/gemini-pro-latest",
    "id": "~google/gemini-pro-latest",
    "description": "Google Gemini Pro Latest",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~google/gemini-flash-latest",
    "id": "~google/gemini-flash-latest",
    "description": "Google Gemini Flash Latest (Fast & cost-efficient)",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~z-ai/glm-latest",
    "id": "~z-ai/glm-latest",
    "description": "Z-AI GLM Latest (High-performance reasoning)",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~z-ai/glm-flash-latest",
    "id": "~z-ai/glm-flash-latest",
    "description": "Z-AI GLM Flash Latest (Fast & low-cost)",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~deepseek/deepseek-pro-latest",
    "id": "~deepseek/deepseek-pro-latest",
    "description": "DeepSeek Pro Latest (Flagship reasoning)",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~deepseek/deepseek-flash-latest",
    "id": "~deepseek/deepseek-flash-latest",
    "description": "DeepSeek Flash Latest (Fast & low-cost)",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  },
  {
    "name": "~moonshotai/kimi-latest",
    "id": "~moonshotai/kimi-latest",
    "description": "Moonshot AI Kimi Latest",
    "endpoints": [{"type": "openai", "baseURL": "http://127.0.0.1:8080/v1"}],
    "parameters": {"temperature": 0.1, "max_new_tokens": 8192}
  }
]'
EOF

# 5. Generate native SvelteKit/dotenv wrapper to run the production build
cat << 'EOF' > run.js
import dotenv from 'dotenv';
import http from 'http';
dotenv.config({ path: '.env' });
dotenv.config({ path: '.env.local', override: true });

// Bind SvelteKit to an internal port
process.env.HOST = '127.0.0.1';
process.env.PORT = '7861';

// Instruct SvelteKit to trust forwarding headers for CSRF origin verification
process.env.PROTOCOL_HEADER = 'x-forwarded-proto';
process.env.HOST_HEADER = 'x-forwarded-host';

// Boot SvelteKit in the background
import('./build/index.js');

// Create a lightweight, zero-dependency reverse proxy on Port 7860
const gateway = http.createServer((req, res) => {
  // Direct webhook requests to FastAPI (8080), other requests to SvelteKit (7861)
  const isFastAPI = req.url.startsWith('/webhook') || 
                    req.url.startsWith('/download') || 
                    req.url.startsWith('/action') || 
                    req.url.startsWith('/v1');
  const targetPort = isFastAPI ? 8080 : 7861;
  const targetHost = '127.0.0.1';

  const headers = { ...req.headers };
  // Populates forwarding headers for SvelteKit CSRF origin comparison
  if (!headers['x-forwarded-host']) {
    headers['x-forwarded-host'] = req.headers['host'] || '127.0.0.1:7860';
  }
  if (!headers['x-forwarded-proto']) {
    headers['x-forwarded-proto'] = 'http';
  }
  headers['host'] = `${targetHost}:${targetPort}`;

  const proxyReq = http.request({
    host: targetHost,
    port: targetPort,
    path: req.url,
    method: req.method,
    headers: headers,
    timeout: 30000
  }, (proxyRes) => {
    res.writeHead(proxyRes.statusCode, proxyRes.headers);
    proxyRes.pipe(res, { end: true });
  });

  req.pipe(proxyReq, { end: true });

  proxyReq.on('timeout', () => {
    console.error(`Gateway proxy timeout routing to ${targetPort}`);
    proxyReq.destroy();
    if (!res.headersSent) {
      res.writeHead(504);
      res.end('Gateway Gateway Timeout');
    }
  });

  proxyReq.on('close', () => {
    proxyReq.destroy();
  });

  proxyReq.on('error', (err) => {
    console.error(`Gateway proxy error routing to ${targetPort}:`, err.message);
    proxyReq.destroy();
    if (!res.headersSent) {
      res.writeHead(502);
      res.end('Gateway routing exception / Destination port offline');
    }
  });
});

gateway.listen(7860, '0.0.0.0', () => {
  console.log('🚀 Micro-Gateway listening on public port 7860 (routing to SvelteKit and FastAPI)...');
});
EOF

if [ -n "$HEADLESS_COMMAND" ]; then
  echo "🤖 Starting Gateway in background for Headless CI execution..."
  node --dns-result-order=ipv4first run.js &
  sleep 2
  echo "🚀 Executing Headless Command: $HEADLESS_COMMAND"
  RESPONSE=$(curl -sN -X POST http://127.0.0.1:8080/v1/chat/completions \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer sk-dummy-key" \
    -d "{\"model\": \"${PREFERRED_MODEL:-project-blue-morpho}\", \"messages\": [{\"role\": \"user\", \"content\": \"$HEADLESS_COMMAND\"}]}")
  echo "$RESPONSE"
  if echo "$RESPONSE" | grep -q "❌"; then
    echo "❌ Headless execution reported an error."
    exit 1
  fi
  echo "✅ Headless execution completed successfully."
  exit 0
else
  echo "Starting Chat-UI on Port 7860..."
  node --dns-result-order=ipv4first run.js
fi
