FROM alpine:edge
RUN apk --no-cache add build-base rustup openssl-dev
RUN rustup-init -y
WORKDIR /app
COPY ./ /app
RUN source "$HOME/.cargo/env" && cargo build --release

FROM alpine:edge
RUN apk --no-cache add imagemagick
RUN magick --version
COPY --from=0 /app/target/release/yellhole .
ENTRYPOINT ["/yellhole"]