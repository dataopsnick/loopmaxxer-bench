# ----------------------------------------
# Stage 1: Build Alibaba Open Code Review (Requires Go 1.25+)
# ----------------------------------------
FROM golang:alpine AS ocr-builder
WORKDIR /app
RUN apk add --no-cache make git
RUN git clone https://github.com/alibaba/open-code-review.git .
RUN make build

# ----------------------------------------
# Stage 2: Build HuggingFace Chat-UI
# ----------------------------------------
FROM node:20-slim AS chatui-builder
WORKDIR /app
RUN apt-get update && apt-get install -y git
RUN git clone https://github.com/huggingface/chat-ui.git .
# Dummy DB URL required to pass SvelteKit validation during build
RUN echo "MONGODB_URL=mongodb://127.0.0.1:27017/chatui" > .env.local
# Copy the GIF pre-stored in your Space repository into SvelteKit's static assets
COPY omni-welcome.gif static/huggingchat/omni-welcome.gif
COPY omni-welcome.gif static/chatui/omni-welcome.gif
RUN npm ci
RUN npm run build

# ----------------------------------------
# Stage 3: Extract MongoDB
# ----------------------------------------
FROM mongo:7 AS mongo

# ----------------------------------------
# Stage 4: Final Runtime (Free CPU Tier)
# ----------------------------------------
FROM node:20-slim

# Install Python, MongoDB helper utilities, and critical native libraries (libgomp1/libcurl4)
RUN apt-get update && apt-get install -y \
    python3 \
    python3-venv \
    curl \
    ca-certificates \
    libgomp1 \
    libcurl4 \
    dnsutils \
    procps \
    git \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Copy the pre-compiled MongoDB binaries directly from the official image
COPY --from=mongo /usr/bin/mongo* /usr/bin/

# Set up non-root user (UID 1011) for Hugging Face Spaces compatibility
RUN useradd -m -u 1011 user
USER user
ENV HOME=/home/user
WORKDIR $HOME/app

# Set up Python virtual environment to avoid PEP 668 conflicts
RUN python3 -m venv $HOME/venv
RUN $HOME/venv/bin/pip install fastapi uvicorn httpx

# Copy the compiled OCR binary
COPY --from=ocr-builder /app/dist/opencodereview ./ocr

# Copy the compiled Chat-UI
COPY --from=chatui-builder --chown=1011:1011 /app $HOME/chat-ui

# Copy our proxy and startup scripts
COPY --chown=1011:1011 generate_tasklist.py ./generate_tasklist.py
COPY --chown=1011:1011 proxy.py ./proxy.py
COPY --chown=1011:1011 start.sh ./start.sh
RUN chmod +x ./start.sh

EXPOSE 7860

CMD ["./start.sh"]