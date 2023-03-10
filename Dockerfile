# Create a Rust builder with stable Rust. Disable static linking of musl because it segfaults.
FROM alpine:3.17 AS rust-base
RUN apk --no-cache add build-base rustup
RUN rustup-init -y
ENV RUSTFLAGS="-C target-feature=-crt-static"
ENV PATH="/root/.cargo/bin:$PATH" 

# Build our app in release mode.
FROM rust-base AS rust-builder
WORKDIR /app
COPY ./ /app
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo build --release

# Create a deployable image from base Alpine with ImageMagick, and SQLite (for admin stuff), set to
# my time zone, with just the compiled binary.
FROM alpine:3.17
RUN apk --no-cache add imagemagick sqlite tzdata && \
    cp /usr/share/zoneinfo/America/Denver /etc/localtime && \
    echo "America/Denver" > /etc/timezone && \
    apk del tzdata
COPY --from=rust-builder /app/target/release/yellhole .
ENTRYPOINT ["/yellhole"]
