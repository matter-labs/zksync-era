# Use the official Rust image to build the application
FROM rust:latest AS builder

# Set the working directory inside the container
WORKDIR /usr/src/zksync

# Copy the Cargo.toml and Cargo.lock files to the container
COPY ./retrieve-blobs .

# Fetch and build the project dependencies (this will cache the dependencies)
RUN cargo fetch

# Build the application in release mode
RUN cargo build --release

# Use a smaller image for runtime
FROM ubuntu:latest

# Install necessary dependencies for running the application (like PostgreSQL client)
RUN apt-get update && apt-get install -y \
    libpq-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory inside the runtime container
WORKDIR /usr/src/zksync

# Copy the built application from the builder image
COPY --from=builder /usr/src/zksync/target/release/retrieve-blobs .

# Expose the port for the Prometheus metrics server
EXPOSE 7070

# Set the environment variable to run the application with correct permissions
ENV RUST_LOG=info
