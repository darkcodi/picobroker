# PicoBroker

[![crates.io](https://img.shields.io/crates/v/picobroker.svg)](https://crates.io/crates/picobroker)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub](https://img.shields.io/badge/GitHub-darkcodi%2Fpicobroker-blue)](https://github.com/darkcodi/picobroker)

A **minimal MQTT 3.1.1 broker** for embedded systems (no_std) and standard environments.

## Features

- ✅ **no_std** - Fully embedded, no standard library (core library)
- ✅ **MQTT 3.1.1** compliant - Protocol specification compliant
- ✅ **QoS 0** - Fire and forget messaging
- ✅ **QoS 1 (best-effort)** - Accepts QoS 1 and sends PUBACK, but no retry/store-and-forward
- ✅ **Topic wildcards** - Supports `+` (single-level) and `#` (multi-level) wildcards
- ✅ **Heapless (Embassy)** - Zero heap usage on embedded with Embassy
- ✅ **Platform-agnostic** - Works with Tokio (std) or Embassy (embedded)
- ✅ **Configurable** - Compile-time configuration via const generics

## Limitations

For maximum minimalism:

- **QoS 0 only** - Full QoS 1/2 with retry/store-and-forward not implemented
- **No retained messages** - Retain flag not handled
- **No authentication/authorization** - No username/password or ACL support
- **No TLS/SSL** - Plain TCP only

### Default Limits (configurable via const generics)

| Parameter | Default | Description |
|-----------|---------|-------------|
| MAX_TOPICS | 32 | Maximum unique topics system-wide |
| MAX_SESSIONS | 4 | Maximum concurrent client sessions |
| MAX_SUBSCRIBERS_PER_TOPIC | 8 | Maximum clients subscribed to one topic |
| MAX_TOPIC_NAME_LENGTH | 64 | Maximum bytes in topic name |
| MAX_PAYLOAD_SIZE | 256 | Maximum bytes in message payload |

## Protocol Support

| Packet Type | Supported | Notes                                   |
|-------------|-----------|-----------------------------------------|
| CONNECT     | ✅         | MQTT 3.1.1 protocol                    |
| CONNACK     | ✅         | Always accepts connections              |
| PUBLISH     | ✅         | QoS 0 fully, QoS 1 best-effort (PUBACK) |
| SUBSCRIBE   | ✅         | Supports wildcards (`+`, `#`)           |
| SUBACK      | ✅         | Grants requested QoS level              |
| PUBACK      | ✅         | Sent for QoS 1+ publishes               |
| PINGREQ     | ✅         | Keep-alive ping                         |
| PINGRESP    | ✅         | Ping response                           |
| DISCONNECT  | ✅         | Graceful disconnect                     |
| Others      | ❌         | QoS 2 handshake, retained messages, etc.|

## Examples

See the `examples/` directory for complete examples:

- **`tokio_mqtt_server`** - Full Tokio-based MQTT broker server
- **`rp2040_mqtt_server`** - Complete MQTT broker for Raspberry Pi Pico W with WiFi

### Running the Tokio Example

```bash
cd examples/tokio_mqtt_server
cargo run
```

### Running the RP2040 Example

```bash
cd examples/rp2040_mqtt_server
# Set your WiFi credentials as environment variables
export WIFI_SSID="YourNetwork"
export WIFI_PASSWORD="YourPassword"
cargo build --release
# Flash to your RP2040 device with probe-rs or elf2uf2-rs
```

## License

MIT
