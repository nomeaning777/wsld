FROM rust:1.87.0 as builder

RUN apt-get update && apt-get install -y mingw-w64 && apt-get clean && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-pc-windows-gnu
RUN rustup target add x86_64-unknown-linux-musl

COPY . .

RUN cargo build --target x86_64-pc-windows-gnu --release --bin wsldhost
RUN cargo build --target x86_64-unknown-linux-musl --release --bin wsld

FROM scratch

COPY --from=builder /target/x86_64-pc-windows-gnu/release/wsldhost.exe /wsldhost.exe
COPY --from=builder /target/x86_64-unknown-linux-musl/release/wsld /wsld
