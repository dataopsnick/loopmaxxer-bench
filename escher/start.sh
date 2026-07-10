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
    "name": "DeepSeek v4 and GLM 5.2 Fusion",
    "id": "deepseekv4glm5.2",
    "description": "OpenRouter Fusion of deepseek-v4-flash, deepseek-v4-pro, z-ai/glm-5.2.",
    "endpoints": [
      {
        "type": "openai",
        "baseURL": "http://127.0.0.1:8080/v1"
      }
    ],
    "parameters": {
      "temperature": 0.1,
      "max_new_tokens": 8192
    }
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

echo "Starting Chat-UI on Port 7860..."
node --dns-result-order=ipv4first run.js
