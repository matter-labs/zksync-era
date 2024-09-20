FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y \
    curl \
    libpq5 \
    ca-certificates \
    && \
    rm -rf /var/lib/apt/lists/*
