# Stage 1: Builder
FROM rust:latest AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    ffmpeg \
    python3 \
    python3-pip && \
    rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/boss-bot

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/

# Copy the source code and build the app
COPY ./src ./src
RUN cargo build --release

# Stage 2: Runtime
FROM rust:latest

# Set environment variables to non-interactive (this prevents some apt warnings)
ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    tzdata \
    wget \
    libssl-dev \
    ffmpeg \
    python3 \
    python3-pip && \
    rm -rf /var/lib/apt/lists/*

# Download and install yt-dlp
RUN wget -O /usr/local/bin/yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp && \
    chmod a+rx /usr/local/bin/yt-dlp

# Copy the binary from the builder stage
COPY --from=builder /usr/src/boss-bot/target/release/boss-bot /usr/local/bin

# Copy the .env and config.json files to the current working directory of the binary
COPY .env /usr/local/bin/.env
COPY config.json /usr/local/bin/config.json

# Set working directory to where the .env, config.json, and binary are located
WORKDIR /usr/local/bin

# Run the bot
CMD ["./boss-bot"]
