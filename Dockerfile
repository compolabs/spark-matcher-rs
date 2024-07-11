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
    CONTRACT_ID="lalala" \
    INDEXER_URL="http://13.49.144.58:8080/v1/graphql" \
    WEBSOCKET_URL="ws://localhost:8080/v1/graphql" \
    FETCH_ORDER_LIMIT=14 \
    MARKET="BTC" \
    LOG_FILE="matcher.log" \
    FILE_LOG_LEVEL="debug" \
    CONSOLE_LOG_LEVEL="info" \
    MAX_FAIL_COUNT=3 \
    PRIVATE_KEY="lalala2"

EXPOSE 5003

CMD ["matcher"]
