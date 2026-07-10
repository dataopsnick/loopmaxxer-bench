# ----------------------------------------
# Stage 1: Build Alibaba Open Code Review (Requires Go 1.25+)
# ----------------------------------------
FROM golang:1.23-alpine AS ocr-builder
WORKDIR /app
RUN apk add --no-cache make git
RUN git clone https://github.com/alibaba/open-code-review.git . && git checkout v0.1.0
RUN make build

# ----------------------------------------
# Stage 2: Build HuggingFace Chat-UI
# ----------------------------------------
FROM node:20-slim AS chatui-builder
WORKDIR /app
RUN apt-get update && apt-get install -y git
RUN git clone https://github.com/huggingface/chat-ui.git . && git checkout 6b485d41fbca5ccf9b3beea0d319a97ccfcab3df
# Dummy DB URL required to pass SvelteKit validation during build
RUN echo "MONGODB_URL=mongodb://127.0.0.1:27017/chatui" > .env.local
# Copy the GIF pre-stored in your Space repository into SvelteKit's static assets
COPY omni-welcome.gif static/huggingchat/omni-welcome.gif
COPY omni-welcome.gif static/chatui/omni-welcome.gif
RUN npm ci
RUN npm run build

# ----------------------------------------
# Stage 3: Final Runtime (Free CPU Tier)
# ----------------------------------------
FROM node:20-slim

# Install Python, gnupg, curl, and GPG for repo keys, plus MongoDB helper utilities and critical native libraries
RUN apt-get update && apt-get install -y \
    python3 \
    python3-venv \
    curl \
    gnupg \
    ca-certificates \
    libgomp1 \
    libcurl4 \
    dnsutils \
    procps \
    git \
    && curl -fsSL https://www.mongodb.org/static/pgp/server-7.0.asc | gpg --dearmor -o /usr/share/keyrings/mongodb-server-7.0.gpg \
    && echo "deb [ signed-by=/usr/share/keyrings/mongodb-server-7.0.gpg ] http://repo.mongodb.org/apt/debian bookworm/mongodb-org/7.0 main" | tee /etc/apt/sources.list.d/mongodb-org-7.0.list \
    && apt-get update && apt-get install -y mongodb-org-server \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Set up non-root user (UID 1011) for Hugging Face Spaces compatibility
RUN mkdir -p /home/user/app && chown -R 1011:1011 /home/user
RUN useradd -m -u 1011 user || true
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

# Use the clean, standardized local build context copier
COPY --chown=1011:1011 . $HOME/app/
RUN chmod +x $HOME/app/start.sh

EXPOSE 7860

CMD ["./start.sh"]
