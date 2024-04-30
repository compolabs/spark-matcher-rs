# Use the official Rust image for the build stage
FROM rust:1.77 as builder
WORKDIR /usr/src/spark-matcher-rs

# Copy the source code into the container
COPY . .

# Build the application
RUN cargo build --release

# Use Debian slim for the runtime stage
FROM ubuntu:22.04

# Avoid prompts from apt
ENV DEBIAN_FRONTEND=noninteractive
WORKDIR /root/

# Copy the built executable and any other necessary files
COPY --from=builder /usr/src/spark-matcher-rs/target/release/spark-matcher .

# Install any runtime dependencies
RUN apt-get update && apt-get install -y ubuntu-server libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

# Environment variables
ENV PRIVATE_KEY="0x2548b84e9c1581e2c379b84393464e282b7751c2e15a231b0fe7fe3a708f8fd3"
ENV CONTRACT_ID="0xab3fad25cf3b15a21a8b2a1e4a0e2bfc95885d81deb482bd4db6f5f06f0248e8"
ENV INDEXER_URL_NO_ORDER_TYPE="http://localhost:3000/spot/orders?baseToken=0xeb13723dc4e1e325fde9daf552fd829422007332713fd416f9c98a2d775125c7&limit=100&isOpened=true"

# Expose the port the server listens on
EXPOSE 5003

# Command to run the executable
CMD ["./spark-matcher"]