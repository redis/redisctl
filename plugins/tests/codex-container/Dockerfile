# Test fixture only: not the plugin's binary delivery or trust contract.
# Build with --platform linux/amd64; this release has no ARM Linux artifact.
FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293
RUN apk add --no-cache python3 xz libgcc libstdc++
RUN npm install --global @openai/codex@0.160.1
WORKDIR /opt/test-binary
RUN wget -q https://github.com/redis/redisctl/releases/download/redisctl-mcp-v0.12.1/redisctl-mcp-x86_64-unknown-linux-musl.tar.xz \
 && python3 -c "import hashlib,pathlib; assert hashlib.sha256(pathlib.Path('redisctl-mcp-x86_64-unknown-linux-musl.tar.xz').read_bytes()).hexdigest() == '87e13d38d152a111cecb74ba486589cea89d345d8ba4be160b3eb025d99b43f3'" \
 && tar -xJf redisctl-mcp-x86_64-unknown-linux-musl.tar.xz \
 && find /opt/test-binary -name redisctl-mcp -type f -exec cp '{}' /usr/local/bin/redisctl-mcp \;
WORKDIR /fixture
