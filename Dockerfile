
FROM alpine:latest

RUN apk add --no-cache rust cargo musl-dev libpq-dev build-base
# RUN apt update && apt upgrade -y #&& apt install libpq-dev build-essential

WORKDIR /usr/src/libpq-rs

COPY src src
COPY Cargo.toml .
COPY Cargo.lock .
COPY build.rs .
COPY Makefile .

RUN make build