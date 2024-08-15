FROM rust:1.60 as builder
WORKDIR /usr/src/matcher
COPY . .
RUN apt-get update && apt-get install -y openssl libssl-dev cmake pkg-config

RUN cargo build --release
FROM debian:buster-slim

RUN apt-get update && apt-get install -y openssl libssl1.1 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/matcher/target/release/matcher /usr/local/bin/matcher

ENV NODE_ENV=development \
    PORT=5003 \
    WEBSOCKET_URL="ws://localhost:8080/v1/graphql" \
    LOG_FILE="matcher.log" \
    FILE_LOG_LEVEL="debug" \
    CONSOLE_LOG_LEVEL="info" \
    FETCH_ORDER_LIMIT=14 \
    MARKET="BTC" \
    MAX_FAIL_COUNT=3 \
    PRIVATE_KEY="lalala2" \
    MNEMONIC="your mnemonic"  \
    CONTRACT_ID="lalala"

EXPOSE 5003

CMD ["matcher"]
