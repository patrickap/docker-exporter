FROM rust:1.76.0-slim as builder

WORKDIR /build
COPY . .

RUN cargo build --release

FROM debian:12.5-slim

ARG UID="1234" \
    GID="1234"

ENV UID=$UID \
    GID=$GID

COPY --from=builder /build/entrypoint.sh /usr/bin/entrypoint.sh
COPY --from=builder /build/target/release/docker-exporter /usr/bin/docker-exporter

RUN apt-get update \
    && apt-get install -y \
    netcat-openbsd=1.219-1 \
    && groupadd -r -g $GID dex \
    && useradd -r -M -s /bin/sh -u $UID -g dex dex \
    && chmod +x /usr/bin/entrypoint.sh

ENTRYPOINT ["entrypoint.sh"]
CMD ["docker-exporter"]
