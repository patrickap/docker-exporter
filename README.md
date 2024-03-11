# docker-exporter

A Prometheus metrics collector for Docker, similar to `node-exporter` but designed for containerized environments, offering a simpler alternative to `cadvisor` with lower resource consumption.

## Available metrics

- `docker_exporter_state_running_boolean`
- `docker_exporter_cpu_utilization_percent`
- `docker_exporter_memory_usage_bytes`
- `docker_exporter_memory_bytes_total`
- `docker_exporter_memory_utilization_percent`
- `docker_exporter_block_io_tx_bytes_total`
- `docker_exporter_block_io_rx_bytes_total`
- `docker_exporter_network_tx_bytes_total`
- `docker_exporter_network_rx_bytes_total`
