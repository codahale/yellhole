FROM alpine:edge
RUN apk --no-cache add build-base rustup openssl-dev
RUN rustup-init -y
WORKDIR /app
COPY ./ /app
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN source "$HOME/.cargo/env" && cargo build --release

FROM alpine:edge
RUN apk --no-cache add imagemagick openssl sqlite tzdata
RUN echo "America/Denver" > /etc/timezone
RUN apk del tzdata
COPY --from=0 /app/target/release/yellhole .
ENTRYPOINT ["/yellhole"]