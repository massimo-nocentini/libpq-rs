
FROM rust:latest

RUN apt-get update && apt-get install -y libpq-dev build-essential libclang-dev && cargo install bindgen-cli

WORKDIR /usr/src/libpq-rs

COPY src src
COPY Cargo.toml .
COPY Cargo.lock .
COPY build.rs .
COPY Makefile .

RUN make build