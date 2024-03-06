FROM rust:1.76.0-slim as builder

WORKDIR /build
COPY . .

RUN cargo build --release

FROM alpine:3.19.1

ARG UID="1234" \
    GID="1234"

ENV UID=$UID \
    GID=$GID

COPY --from=builder /build/target/release/docker-exporter /usr/bin/docker-exporter
COPY --from=builder /build/entrypoint.sh /usr/bin/entrypoint.sh

RUN apk update \
    && apk add \
      shadow~=4.14.2 \
      su-exec~=0.2 \
    && addgroup -S -g $GID dex \
    && adduser -S -H -D -s /bin/sh -u $UID -G dex dex \
    && chmod +x /usr/bin/entrypoint.sh

ENTRYPOINT ["entrypoint.sh"]
CMD ["docker-exporter"]
