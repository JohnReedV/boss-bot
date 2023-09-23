# Use an official Rust runtime as a parent image
FROM rust:latest as builder

# Install system dependencies
RUN apt-get update && apt-get install -y cmake ffmpeg && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/boss-bot

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/

# Copy the source code and build the app
COPY ./src ./src
RUN cargo build --release

# Runtime Image
FROM rust:latest

# Set environment variables to non-interactive (this prevents some apt warnings)
ENV DEBIAN_FRONTEND=noninteractive

# Install dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates tzdata wget libssl-dev ffmpeg && \
    rm -rf /var/lib/apt/lists/*

# Download and install yt-dlp
RUN wget -O /usr/local/bin/yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp && \
    chmod a+rx /usr/local/bin/yt-dlp

# Copy the binary to /usr/local/bin
COPY --from=builder /usr/src/boss-bot/target/release/boss-bot /usr/local/bin

# Copy the .env file to the current working directory of the binary
COPY .env /usr/local/bin/.env

# Set working directory to where the .env and binary are located
WORKDIR /usr/local/bin

# Run the bot
CMD ["./boss-bot"]
