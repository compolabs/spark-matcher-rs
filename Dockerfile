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
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

# Expose the port the server listens on
EXPOSE 5003
EXPOSE 3000
EXPOSE 5432

# Command to run the executable
CMD ["./spark-matcher"]