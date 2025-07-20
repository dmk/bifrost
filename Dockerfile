# Multi-stage build for optimal image size
FROM rust:1.85-slim-bullseye AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application in release mode
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -r -s /bin/false bifrost

# Create directory for configuration
RUN mkdir -p /app/examples
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/bifrost /app/bifrost

# Copy example configurations
COPY examples/ ./examples/

# Change ownership
RUN chown -R bifrost:bifrost /app

# Switch to non-root user
USER bifrost

# Expose the default port
EXPOSE 11211

ENTRYPOINT [ "./bifrost" ]

# Default command
CMD ["--config", "/app/config.yaml"]
