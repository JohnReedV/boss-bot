# Use an official Rust runtime as a parent image
FROM rust:latest AS builder

# Install system dependencies required for the release build
RUN apt-get update && \
    apt-get install -y --no-install-recommends cmake ca-certificates pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/boss-bot

# Copy manifests first so dependency resolution is explicit in the image build
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/

# Copy the source code and build the app
COPY ./src ./src
RUN cargo build --release

# Runtime image
FROM rust:latest

# Set environment variables to non-interactive (this prevents some apt warnings)
ENV DEBIAN_FRONTEND=noninteractive

# Install runtime dependencies for the yt-dlp -> ffmpeg -> Songbird audio pipeline
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates tzdata wget python3 ffmpeg openssl && \
    rm -rf /var/lib/apt/lists/*

# Download and verify yt-dlp alongside ffmpeg so audio failures surface during image build
RUN wget -O /usr/local/bin/yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp && \
    chmod a+rx /usr/local/bin/yt-dlp && \
    yt-dlp --version && \
    ffmpeg -version

# Copy the binary to /usr/local/bin
COPY --from=builder /usr/src/boss-bot/target/release/boss-bot /usr/local/bin

# Copy the .env file to the current working directory of the binary
COPY .env /usr/local/bin/.env

# Set working directory to where the .env and binary are located
WORKDIR /usr/local/bin

# Run the bot
CMD ["./boss-bot"]
