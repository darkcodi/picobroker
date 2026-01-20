# PicoBroker

A **minimal MQTT 3.1.1 broker** for embedded systems (no_std) using the Embassy async framework.

## Features

- ✅ **no_std** - Fully embedded, no standard library
- ✅ **MQTT 3.1.1** compliant - Protocol specification compliant
- ✅ **QoS 0** - Fire and forget messaging
- ✅ **Heapless** - All stack/static allocation (zero heap usage)
- ✅ **Generic networking** - Works with any Embassy network stack
- ✅ **Configurable** - Compile-time configuration via const generics

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
