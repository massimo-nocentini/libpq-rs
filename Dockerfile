
FROM rust:alpine

RUN apk add postgresql-dev build-base

WORKDIR /usr/src/libpq-rs

COPY src src
COPY Cargo.toml .
COPY Cargo.lock .
COPY build.rs .
COPY Makefile .

RUN make build