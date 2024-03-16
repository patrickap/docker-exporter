# docker-exporter

A Prometheus metrics collector for Docker, similar to `node-exporter` but designed for containerized environments, offering a simpler alternative to `cadvisor` with lower resource consumption.

## Available metrics

- `docker_exporter_state_running_boolean`
- `docker_exporter_cpu_utilization_percent`
- `docker_exporter_memory_usage_bytes`
- `docker_exporter_memory_limit_bytes`
- `docker_exporter_memory_utilization_percent`
- `docker_exporter_block_io_tx_bytes_total`
- `docker_exporter_block_io_rx_bytes_total`
- `docker_exporter_network_tx_bytes_total`
- `docker_exporter_network_rx_bytes_total`

## Getting started

To get started with Docker-Exporter, follow these steps:

1. Pull the Docker-Exporter image from the official Docker Hub repository and run the container with the specified configurations:

```bash
docker run -d \
  --name docker-exporter \
  --restart always \
  -p 9630:9630 \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  patrickap/docker-exporter:latest
```

Alternatively, you can use Docker Compose:

```yml
version: "3.7"

services:
  docker-exporter:
    image: patrickap/docker-exporter:latest
    restart: always
    ports: 9630:9630
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
```

2. The metrics are now available at `localhost:9630/metrics`
