use bytes::{Buf, Bytes, BytesMut};
use log::error;
use log::info;
use picobroker::client::ClientId;
use picobroker::protocol::heapless::HeaplessVec;
use picobroker::protocol::packets::{
    ConnectPacket, ConnAckPacket, DisconnectPacket, Packet, PacketEncoder, PacketTypeDynamic,
    PublishPacket,
};
use picobroker::protocol::ProtocolError;
use picobroker::topics::TopicName;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const MAX_TOPIC_NAME_LENGTH: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;

// Configuration (hardcoded for this example)
const BROKER_ADDR: &str = "127.0.0.1:1883";
const CLIENT_ID: &str = "tokio-client";
const TOPIC: &str = "test/topic";
const MESSAGE: &str = "Hello from picobroker client!";
const KEEP_ALIVE: u16 = 60;

/// Parse MQTT variable length integer from buffer
fn parse_remaining_length(buffer: &BytesMut) -> Result<(usize, usize), ProtocolError> {
    let mut idx = 1;
    let mut multiplier = 1usize;
    let mut value = 0usize;

    loop {
        if idx >= buffer.len() {
            return Err(ProtocolError::IncompletePacket {
                available: buffer.len(),
            });
        }
        let byte = buffer[idx] as usize;
        idx += 1;
        value += (byte & 0x7F) * multiplier;

        if (byte & 0x80) == 0 {
            break;
        }

        multiplier *= 128;
        if multiplier > 128 * 128 * 128 {
            return Err(ProtocolError::InvalidPacketLength {
                expected: 4,
                actual: 5,
            });
        }
    }

    Ok((value, idx - 1))
}

/// Encode a packet into Bytes for transmission
fn encode_packet(
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
) -> Result<Bytes, std::io::Error> {
    let buffer_size = 5 + MAX_PAYLOAD_SIZE + MAX_TOPIC_NAME_LENGTH;
    let mut buffer = vec![0u8; buffer_size];

    let size = packet
        .encode(&mut buffer)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to encode packet"))?;

    Ok(BytesMut::from(&buffer[..size]).freeze())
}

/// Read a complete MQTT packet from the stream
async fn read_packet(
    socket: &mut TcpStream,
    buffer: &mut BytesMut,
) -> Result<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, Box<dyn std::error::Error>> {
    // Ensure we have at least 2 bytes (packet type + min remaining length)
    while buffer.len() < 2 {
        buffer.reserve(2);
        let n = socket.read_buf(buffer).await?;
        if n == 0 {
            return Err("Connection closed by server".into());
        }
    }

    // Parse remaining length
    let (remaining_len, var_len_bytes) = parse_remaining_length(buffer)?;
    let total_len = 1 + var_len_bytes + remaining_len;

    // Ensure we have the full packet
    while buffer.len() < total_len {
        buffer.reserve(total_len - buffer.len());
        let n = socket.read_buf(buffer).await?;
        if n == 0 {
            return Err("Connection closed by server".into());
        }
    }

    // Decode the packet
    let packet = Packet::decode(&buffer[..total_len])?;

    // Advance buffer (keep any excess for next packet)
    buffer.advance(total_len);

    Ok(packet)
}

struct MqttClient {
    socket: TcpStream,
    buffer: BytesMut,
}

impl MqttClient {
    /// Connect to the broker and return a connected client
    async fn connect(
        broker_addr: &str,
        client_id: &str,
        keep_alive: u16,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Connecting to broker at {}", broker_addr);

        let socket = TcpStream::connect(broker_addr).await?;
        let buffer = BytesMut::new();

        let mut client = MqttClient { socket, buffer };

        // Create and send CONNECT packet
        use picobroker::protocol::packets::ConnectFlags;
        let client_id_heapless = ClientId::try_from(client_id)?;

        let connect_packet = ConnectPacket {
            connect_flags: ConnectFlags::CLEAN_SESSION,
            keep_alive,
            client_id: client_id_heapless,
            will_topic: None,
            will_payload: None,
            username: None,
            password: None,
        };

        let packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> =
            Packet::Connect(connect_packet);
        let encoded = encode_packet(&packet)?;

        info!("Sending CONNECT packet");
        client.socket.write_all(&encoded).await?;

        // Wait for CONNACK
        info!("Waiting for CONNACK...");
        let connack = client.read_connack().await?;
        match connack.return_code {
            picobroker::protocol::packets::ConnectReturnCode::Accepted => {
                info!(
                    "Connected successfully! Session present: {}",
                    connack.session_present
                );
            }
            _ => {
                error!("Connection failed: {:?}", connack.return_code);
                return Err(format!("Connection rejected: {:?}", connack.return_code).into());
            }
        }

        Ok(client)
    }

    /// Read a CONNACK packet from the stream
    async fn read_connack(&mut self) -> Result<ConnAckPacket, Box<dyn std::error::Error>> {
        let packet = read_packet(&mut self.socket, &mut self.buffer).await?;

        match packet {
            Packet::ConnAck(connack) => Ok(connack),
            _ => Err(format!("Expected CONNACK, got {:?}", packet.packet_type()).into()),
        }
    }

    /// Publish a message to a topic
    async fn publish(
        &mut self,
        topic: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Publishing to topic '{}': {}", topic, message);

        use picobroker::protocol::qos::QoS;
        let topic_name = TopicName::try_from(topic)?;

        let mut payload = HeaplessVec::<u8, MAX_PAYLOAD_SIZE>::new();
        payload
            .extend_from_slice(message.as_bytes())
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Message too large for MAX_PAYLOAD_SIZE",
                )
            })?;

        let publish_packet = PublishPacket {
            topic_name,
            packet_id: None, // QoS 0, no packet ID
            payload,
            qos: QoS::AtMostOnce,
            dup: false,
            retain: false,
        };

        let packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> =
            Packet::Publish(publish_packet);
        let encoded = encode_packet(&packet)?;

        self.socket.write_all(&encoded).await?;
        self.socket.flush().await?;

        info!("Message published");
        Ok(())
    }

    /// Disconnect from the broker
    async fn disconnect(mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Disconnecting from broker");

        let disconnect_packet = DisconnectPacket;
        let packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> =
            Packet::Disconnect(disconnect_packet);
        let encoded = encode_packet(&packet)?;

        self.socket.write_all(&encoded).await?;
        self.socket.shutdown().await?;

        info!("Disconnected");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut client = MqttClient::connect(BROKER_ADDR, CLIENT_ID, KEEP_ALIVE).await?;

    client.publish(TOPIC, MESSAGE).await?;

    client.disconnect().await?;

    Ok(())
}
