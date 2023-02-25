FROM rust:1.67.1-buster as builder
WORKDIR /usr/src/twitter_video_dl
COPY . .
RUN cargo install --path .

FROM debian:buster-slim
RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/twitter_video_dl /usr/local/bin/twitter_video_dl
CMD ["twitter_video_dl"]
