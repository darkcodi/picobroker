# PicoBroker

A **minimal MQTT 3.1.1 broker** for embedded systems (no_std) and standard environments.

## Architecture

PicoBroker is split into three separate crates:

- **`picobroker-core`** - Pure `no_std` core library with only helpful dependencies
- **`picobroker-tokio`** - Tokio runtime support for standard library environments
- **`picobroker-embassy`** - Embassy runtime support for embedded systems (no_std)

## Features

- ✅ **no_std** - Fully embedded, no standard library (core library)
- ✅ **MQTT 3.1.1** compliant - Protocol specification compliant
- ✅ **QoS 0** - Fire and forget messaging
- ✅ **Heapless** - All stack/static allocation (zero heap usage)
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
use picobroker_tokio::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new broker
    let mut broker = TokioPicoBroker::<30, 30, 4, 4, 4>::new_tokio();

    // Register a session (using ClientId for MQTT client identifier)
    let client_id = ClientId::try_from("test_client")?;
    broker.register_new_session(client_id.clone(), 60)?;

    // Use the broker...
    Ok(())
}
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
use picobroker_embassy::*;

#[embassy_executor::main(entry = "main")]
async fn main(sp: embassy_executor::Spawner) {
    // Create a new broker
    let mut broker = EmbassyPicoBroker::<30, 30, 4, 4, 4>::new_embassy();

    // Register a session (using ClientId for MQTT client identifier)
    let client_id = ClientId::try_from("test_client").unwrap();
    broker.register_new_session(client_id.clone(), 60).unwrap();

    // Use the broker...
}
```

**Note:** Embassy networking is currently stubbed out. Enable the `embassy` feature when ready:
```toml
picobroker-embassy = { version = "0.1", features = ["embassy"] }
```

## Limitations

For maximum minimalism:

- QoS 0 only (no ack/retry)
- No topic wildcards (+, #)
- No retained messages
- No authentication/authorization
- No TLS/SSL
- Maximum 32 unique topics system-wide
- Maximum 16 subscriptions per client

## Protocol Support

| Packet Type | Supported | Notes                      |
|-------------|-----------|----------------------------|
| CONNECT     | ✅         | MQTT 3.1.1 protocol        |
| CONNACK     | ✅         | Always accepts connections |
| PUBLISH     | ✅         | QoS 0 only                 |
| SUBSCRIBE   | ✅         | Exact topic matching       |
| SUBACK      | ✅         | Grants QoS 0               |
| DISCONNECT  | ✅         | Graceful disconnect        |
| Others      | ❌         | Not supported              |

## Examples

See the `examples/` directory for complete examples:

- `tokio_broker.rs` - Tokio-based broker example

## License

MIT OR Apache-2.0
