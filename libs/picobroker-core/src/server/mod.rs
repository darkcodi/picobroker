use crate::broker::PicoBroker;
use crate::error::BrokerError;
use crate::protocol::packets::{Packet, PacketEncoder};
use crate::traits::{Delay, NetworkError, TcpListener, TcpStream, TimeSource};
use log::{error, info, warn};

pub mod buffer_manager;
pub mod connection_manager;

pub use buffer_manager::{BufferManager, SessionReadBuffer};
pub use connection_manager::ConnectionManager;

pub struct PicoBrokerServer<
    TS: TimeSource,
    TL: TcpListener,
    D: Delay,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    time_source: TS,
    listener: TL,
    delay: D,
    broker: PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
    connection_manager: ConnectionManager<TL, MAX_SESSIONS>,
    buffer_manager: BufferManager<MAX_PAYLOAD_SIZE, MAX_SESSIONS>,
}

impl<
        TS: TimeSource,
        TL: TcpListener,
        D: Delay,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    >
    PicoBrokerServer<
        TS,
        TL,
        D,
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    pub fn new(time_source: TS, listener: TL, delay: D) -> Self {
        Self {
            time_source,
            listener,
            delay,
            broker: PicoBroker::new(),
            connection_manager: ConnectionManager::new(),
            buffer_manager: BufferManager::new(),
        }
    }

    pub async fn run(&mut self) -> Result<(), BrokerError>
    where
        TL::Stream: Send + 'static,
    {
        info!("Starting server main loop");
        loop {
            self.accept_new_connection().await;

            self.read_session_messages().await;

            self.process_session_messages().await;

            self.write_session_messages().await;

            self.delay.sleep_ms(10).await;
        }
    }

    pub fn cleanup_disconnected(&mut self) {
        for session_id in self
            .broker
            .get_disconnected_sessions()
            .into_iter()
            .flatten()
        {
            info!("Cleaning up disconnected session {}", session_id);
            self.remove_session(session_id);
        }

        let current_time = self.time_source.now_nano_secs();
        for session_id in self
            .broker
            .get_expired_sessions(current_time)
            .into_iter()
            .flatten()
        {
            info!("Cleaning up expired session {}", session_id);
            self.remove_session(session_id);
        }
    }

    pub fn remove_session(&mut self, session_id: u128) {
        match self.broker.remove_session(session_id) {
            true => info!("Removed session {}", session_id),
            false => error!("No session found to remove with id {}", session_id),
        }
        match self.connection_manager.remove_session(session_id) {
            true => info!("Removed connection for session {}", session_id),
            false => error!("No connection found to remove for session {}", session_id),
        }
        match self.buffer_manager.remove_session(session_id) {
            true => info!("Removed buffer for session {}", session_id),
            false => error!("No buffer found to remove for session {}", session_id),
        }
    }

    async fn accept_new_connection(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        if let Ok((mut stream, socket_addr)) = self.listener.try_accept().await {
            info!("Received new connection from {}", socket_addr);

            let current_time = self.time_source.now_nano_secs();
            let session_id = current_time;
            const KEEP_ALIVE_SECS: u16 = 60;

            match self
                .broker
                .register_new_session(session_id, KEEP_ALIVE_SECS, current_time)
            {
                Ok(_) => {
                    info!("Registered new session with id {}", session_id);
                    self.connection_manager.set_stream(session_id, stream);
                }
                Err(e) => {
                    error!("Failed to register new session: {}", e);
                    error!("Closing connection");
                    let _ = stream.close().await;
                }
            }
        }
    }

    async fn read_session_messages(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        for session_id in self.broker.get_all_sessions().into_iter().flatten() {
            if let Err(e) = self.read_single_session(session_id).await {
                match e {
                    BrokerError::SessionDisconnected { session_id: sid } => {
                        info!("Session {} disconnected", sid);
                    }
                    BrokerError::Network(NetworkError::ConnectionClosed) => {
                        info!(
                            "Session {} disconnected due to connection closed",
                            session_id
                        );
                    }
                    _ => {
                        error!("Error reading from session {}: {:?}", session_id, e);
                    }
                }
            }
        }
    }

    async fn read_single_session(&mut self, session_id: u128) -> Result<(), BrokerError>
    where
        TL::Stream: Send + 'static,
    {
        self.ensure_buffer_space(session_id)?;

        let remaining_space = self.buffer_manager.get_remaining_space(session_id);
        let mut read_buf = [0u8; MAX_PAYLOAD_SIZE];

        let bytes_read = {
            let stream = match self.connection_manager.get_stream_mut(session_id) {
                Some(s) => s,
                None => {
                    error!("No stream found for session {}", session_id);
                    let _ = self.broker.mark_session_disconnected(session_id);
                    return Err(BrokerError::SessionDisconnected { session_id });
                }
            };

            match stream.try_read(&mut read_buf[..remaining_space]).await {
                Ok(0) => {
                    info!("Connection closed by peer (0 bytes read)");
                    let _ = self.broker.mark_session_disconnected(session_id);
                    return Err(BrokerError::SessionDisconnected { session_id });
                }
                Ok(n) => {
                    info!("Read {} bytes from session {}", n, session_id);
                    n
                }
                Err(NetworkError::ConnectionClosed) => {
                    error!(
                        "Fatal error reading from session {}: Connection closed",
                        session_id
                    );
                    let _ = self.broker.mark_session_disconnected(session_id);
                    return Err(BrokerError::SessionDisconnected { session_id });
                }
                Err(NetworkError::ReadFailed) => {
                    error!(
                        "Fatal error reading from session {}: Read failed",
                        session_id
                    );
                    let _ = self.broker.mark_session_disconnected(session_id);
                    return Err(BrokerError::SessionDisconnected { session_id });
                }
                Err(_) => 0,
            }
        };

        if bytes_read > 0 {
            let buffer = self.buffer_manager.get_buffer_mut(session_id);
            buffer.append(&read_buf[..bytes_read]);
            self.process_received_data(session_id)?;
        }

        Ok(())
    }

    fn ensure_buffer_space(&mut self, session_id: u128) -> Result<(), BrokerError> {
        let remaining = self.buffer_manager.get_remaining_space(session_id);
        if remaining == 0 {
            warn!(
                "RX buffer overflow for session {}, clearing buffer",
                session_id
            );
            let buffer = self.buffer_manager.get_buffer_mut(session_id);
            buffer.clear();
        }
        Ok(())
    }

    fn process_received_data(&mut self, session_id: u128) -> Result<(), BrokerError> {
        let current_time = self.time_source.now_nano_secs();

        match self
            .buffer_manager
            .try_decode_packet::<MAX_TOPIC_NAME_LENGTH>(session_id)
        {
            Ok(Some(packet)) => {
                info!("Received packet from session {}: {:?}", session_id, packet);
                self.broker
                    .queue_packet_received_from_client(session_id, packet, current_time)?;
            }
            Ok(None) => {}
            Err(e) => {
                error!("Error decoding packet from session {}: {}", session_id, e);
            }
        }

        Ok(())
    }

    async fn process_session_messages(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        match self.broker.process_all_session_packets() {
            Ok(_) => {}
            Err(e) => {
                warn!("Error processing session messages: {:?}", e);
            }
        }
    }

    async fn write_session_messages(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        for session_id in self.broker.get_all_sessions().into_iter().flatten() {
            if let Err(e) = self.write_session_queue(session_id).await {
                warn!("Error writing to session {}: {:?}", session_id, e);
            }
        }
    }

    async fn write_session_queue(&mut self, session_id: u128) -> Result<(), BrokerError>
    where
        TL::Stream: Send + 'static,
    {
        while let Some(packet) = self.broker.dequeue_packet_to_send_to_client(session_id)? {
            if let Err(e) = self.write_single_packet(session_id, packet).await {
                warn!("Error writing packet to session {}: {:?}", session_id, e);
                break;
            }
        }
        Ok(())
    }

    async fn write_single_packet(
        &mut self,
        session_id: u128,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError>
    where
        TL::Stream: Send + 'static,
    {
        info!("Sending packet to session {}: {:?}", session_id, packet);

        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let packet_size = packet.encode(&mut buffer).map_err(|e| {
            error!("Error encoding packet: {}", e);
            BrokerError::Protocol(e)
        })?;
        let encoded_packet = &buffer[..packet_size];
        info!("Encoded packet: {} bytes", packet_size);

        let mut total_written = 0;

        while total_written < packet_size {
            let stream = self
                .connection_manager
                .get_stream_mut(session_id)
                .ok_or_else(|| {
                    error!("No stream found for session {}", session_id);
                    let _ = self.broker.mark_session_disconnected(session_id);
                    BrokerError::SessionDisconnected { session_id }
                })?;

            match stream.write(&encoded_packet[total_written..]).await {
                Ok(0) => {
                    error!("Write returned 0 bytes, connection closed");
                    let _ = self.broker.mark_session_disconnected(session_id);
                    return Err(BrokerError::SessionDisconnected { session_id });
                }
                Ok(bytes_written) => {
                    total_written += bytes_written;
                    info!("Wrote {} bytes (total: {})", bytes_written, total_written);

                    if total_written >= packet_size {
                        let stream = self
                            .connection_manager
                            .get_stream_mut(session_id)
                            .ok_or(BrokerError::SessionDisconnected { session_id })?;

                        stream.flush().await.map_err(|e| {
                            error!("Error flushing stream: {}", e);
                            let _ = self.broker.mark_session_disconnected(session_id);
                            BrokerError::Network(e)
                        })?;
                        break;
                    }
                }
                Err(NetworkError::ConnectionClosed | NetworkError::WriteFailed) => {
                    error!(
                        "Fatal error writing to session {}: Connection closed or write failed",
                        session_id
                    );
                    let _ = self.broker.mark_session_disconnected(session_id);
                    return Err(BrokerError::SessionDisconnected { session_id });
                }
                Err(_) => {
                    continue;
                }
            }
        }

        Ok(())
    }
}
