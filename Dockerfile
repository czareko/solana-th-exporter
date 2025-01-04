# Use an official Rust image as the base
FROM rust:1.81.0 as builder

# Set the working directory
WORKDIR /app

# Copy the entire project into the container
COPY . .

# Build the project in release mode
RUN cargo build --release

# Use a lightweight image for the final stage
FROM debian:bookworm-slim

# Install necessary dependencies for the Rust binary
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/solana-th-exporter /app/solana-th-exporter

# Expose any necessary ports (if your application listens on a port)
EXPOSE 8080

# Command to run the application
CMD ["./solana-th-exporter"]