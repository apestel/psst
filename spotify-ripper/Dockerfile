FROM rust:1.64.0 as builder
RUN apt-get update
RUN apt-get install libssl-dev librust-alsa-sys-dev protobuf-compiler libprotobuf-dev -y
RUN USER=root cargo new --bin spotify-ripper
WORKDIR /spotify-ripper
COPY ./frontend/dist ./dist
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./src ./src
COPY sqlx-data.json .
# RUN RUSTFLAGS=-Clinker=x86_64-linux-gnu-gcc /root/.cargo/bin/cargo build --release --target=x86_64-unknown-linux-musl
RUN cargo install --path .

FROM debian:stable-slim
RUN apt-get update && apt-get install -y ca-certificates librust-alsa-sys-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /spotify-ripper/dist /opt/spotify-ripper/dist
COPY --from=builder /usr/local/cargo/bin/spotify-ripper /opt/spotify-ripper/spotify-ripper
WORKDIR /opt/spotify-ripper
EXPOSE 3000
CMD ["./spotify-ripper"]
