# PicoBroker

A **minimal MQTT 3.1.1 broker** for embedded systems (no_std) and standard environments.

## Architecture

PicoBroker is split into three separate crates:

- **`picobroker-core`** - Pure `no_std` core library with zero dependencies
- **`picobroker-tokio`** - Tokio runtime support for standard library environments
- **`picobroker-embassy`** - Embassy runtime support for embedded systems (no_std)

## Features

- ✅ **no_std** - Fully embedded, no standard library (core library)
- ✅ **MQTT 3.1.1** compliant - Protocol specification compliant
- ✅ **QoS 0** - Fire and forget messaging
- ✅ **QoS 1 (best-effort)** - Accepts QoS 1 and sends PUBACK, but no retry/store-and-forward
- ✅ **Topic wildcards** - Supports `+` (single-level) and `#` (multi-level) wildcards
- ✅ **Heapless (Embassy)** - Zero heap usage on embedded with Embassy
- ✅ **Platform-agnostic** - Works with Tokio (std) or Embassy (embedded)
- ✅ **Configurable** - Compile-time configuration via const generics

## Usage

### Tokio (std)

For standard library environments using Tokio:

```toml
[dependencies]
picobroker-tokio = "0.1"
```

```rust
use picobroker_tokio::DefaultMqttServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new server with default configuration
    let server = DefaultMqttServer::new();

    // Run the broker
    server.run("0.0.0.0:1883").await?;

    Ok(())
}
```

The `MqttServer` type takes const generics for configuration:
```rust
// MqttServer<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE,
//           MAX_SESSIONS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>

// Convenience type aliases are available:
pub type DefaultMqttServer = MqttServer<64, 256, 8, 4, 32, 8>;
pub type SmallMqttServer = MqttServer<32, 128, 4, 2, 16, 4>;
pub type LargeMqttServer = MqttServer<128, 1024, 16, 16, 128, 32>;
```

### Embassy (embedded)

For embedded systems using Embassy:

```toml
[dependencies]
picobroker-embassy = "0.1"
```

```rust,ignore
#![no_std]
#![no_main]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use picobroker_embassy::DefaultMqttServer;
use static_cell::StaticCell;

#[embassy_executor::main]
async fn main(sp: embassy_executor::Spawner) {
    // Static storage (required by Embassy no_std)
    static BROKER_CELL: StaticCell<Mutex<...>> = StaticCell::new();
    static NOTIF_CELL: StaticCell<NotificationRegistry<4, _>> = StaticCell::new();
    static SESSION_ID_GEN_CELL: StaticCell<Mutex<...>> = StaticCell::new();

    // Create server with default configuration
    let server = DefaultMqttServer::<CriticalSectionRawMutex>::new(
        &BROKER_CELL, &NOTIF_CELL, &SESSION_ID_GEN_CELL
    );

    // Spawn accept tasks and run...
}
```

The `MqttServer` type takes a mutex type and const generics:
```rust
// MqttServer<Mutex, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE,
//            MAX_SESSIONS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>

// Convenience type aliases (mutex type parameter required):
pub type DefaultMqttServer<M> = MqttServer<M, 64, 256, 8, 4, 32, 8>;
pub type SmallMqttServer<M> = MqttServer<M, 32, 128, 4, 2, 16, 4>;
pub type LargeMqttServer<M> = MqttServer<M, 128, 1024, 16, 16, 128, 32>;
```

See the `rp2040_mqtt_server` example for a complete working MQTT broker on Raspberry Pi Pico W.

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
cd picobroker-tokio/examples/tokio_mqtt_server
cargo run
```

### Running the RP2040 Example

```bash
cd picobroker-embassy/examples/rp2040_mqtt_server
# Set your WiFi credentials as environment variables
export WIFI_SSID="YourNetwork"
export WIFI_PASSWORD="YourPassword"
cargo build --release
# Flash to your RP2040 device with probe-rs or elf2uf2-rs
```

## License

MIT OR Apache-2.0
