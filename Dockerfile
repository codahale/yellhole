# Create a Rust builder with stable Rust. Disable static linking of musl because it segfaults.
FROM alpine:3.17 AS rust-base
RUN apk --no-cache add build-base rustup git
RUN rustup-init -y
ENV RUSTFLAGS="-C target-feature=-crt-static"
ENV PATH="/root/.cargo/bin:$PATH" 

# Create a dummy app and use it to compile our dependencies in release mode. This app has a lot of
# dependencies but they don't change very often.
FROM rust-base AS rust-deps
WORKDIR /app
RUN USER=root cargo init --bin .
RUN USER=root cargo new --bin xtask 
COPY Cargo.toml /app
COPY Cargo.lock /app
COPY xtask/Cargo.toml /app/xtask
RUN git config --global --add safe.directory /app
RUN cargo build --release

# Build our app in release mode.
FROM rust-deps AS rust-builder
WORKDIR /app
COPY ./ /app
RUN cargo build --release

# Create a deployable image from base Alpine with ffmpeg, ImageMagick, and SQLite (for admin stuff),
# set to # my time zone, with just the compiled binary.
FROM alpine:3.17
RUN apk --no-cache add ffmpeg imagemagick sqlite tzdata && \
    cp /usr/share/zoneinfo/America/Denver /etc/localtime && \
    echo "America/Denver" > /etc/timezone && \
    apk del tzdata
COPY --from=rust-builder /app/target/release/yellhole .
ENTRYPOINT ["/yellhole"]
