FROM rust:1.73.0-buster

RUN apt-get update && apt-get install -y mingw-w64 && apt-get clean && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-pc-windows-gnu
