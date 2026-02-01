#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::info;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_net::tcp::TcpSocket;
use embassy_rp::gpio::Output;
use embassy_rp::interrupt::typelevel::Binding;
use embassy_rp::pio::{InterruptHandler, Pio};
use picobroker_core::broker::PicoBroker;
use picobroker_core::protocol::packets::{Packet, PacketEncoder};
use picobroker_core::protocol::ProtocolError;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

embassy_rp::bind_interrupts!(pub struct Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

const CYW43_FW: &[u8] = include_bytes!("./cyw43-firmware/43439A0.bin");
const CYW43_CLM: &[u8] = include_bytes!("./cyw43-firmware/43439A0_clm.bin");

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

// MQTT Broker Configuration
const MAX_TOPIC_NAME_LENGTH: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;
const QUEUE_SIZE: usize = 8;
const MAX_SESSIONS: usize = 4;        // 4 concurrent sessions
const MAX_TOPICS: usize = 32;
const MAX_SUBSCRIBERS_PER_TOPIC: usize = 8;
const DEFAULT_KEEP_ALIVE: u16 = 60;

// Network Configuration
const MQTT_PORT: u16 = 1883;
const RX_BUFFER_SIZE: usize = 1536;   // Max MQTT packet
const TX_BUFFER_SIZE: usize = 1536;

// =============================================================================
// MQTT Broker Type Aliases and Static State
// =============================================================================

// Type aliases for broker (QoS 0,1 only - no QoS 2 handling needed)
type MqttBroker = PicoBroker<
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;

// Static storage for broker (no Arc in no-std)
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
type BrokerMutex = Mutex<ThreadModeRawMutex, MqttBroker>;

static BROKER_CELL: StaticCell<BrokerMutex> = StaticCell::new();

// Session ID generator
struct SessionIdGen(u64);

impl SessionIdGen {
    fn new() -> Self {
        Self(0)
    }

    fn generate(&mut self) -> u128 {
        let timestamp = embassy_time::Instant::now().as_ticks() as u128;
        let counter = self.0;
        self.0 = self.0.wrapping_add(1);
        (timestamp << 32) | (counter as u128)
    }
}

static SESSION_ID_GEN_CELL: StaticCell<Mutex<ThreadModeRawMutex, SessionIdGen>> = StaticCell::new();

static STACK_CELL: StaticCell<Stack<'static>> = StaticCell::new();

// Helper to get current time in nanoseconds
fn current_time_nanos() -> u128 {
    embassy_time::Instant::now().as_ticks() as u128
}

pub struct WifiPins {
    pub pwr: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_23>,
    pub cs: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_25>,
    pub clk: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_29>,
    pub dio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_24>,
    pub pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
    pub dma: embassy_rp::Peri<'static, embassy_rp::peripherals::DMA_CH0>,
}

pub struct PioKeepalive<'a> {
    _common: embassy_rp::pio::Common<'a, embassy_rp::peripherals::PIO0>,
    _irq_flags: embassy_rp::pio::IrqFlags<'a, embassy_rp::peripherals::PIO0>,
    _irq1: embassy_rp::pio::Irq<'a, embassy_rp::peripherals::PIO0, 1>,
    _irq2: embassy_rp::pio::Irq<'a, embassy_rp::peripherals::PIO0, 2>,
    _irq3: embassy_rp::pio::Irq<'a, embassy_rp::peripherals::PIO0, 3>,
    _sm1: embassy_rp::pio::StateMachine<'a, embassy_rp::peripherals::PIO0, 1>,
    _sm2: embassy_rp::pio::StateMachine<'a, embassy_rp::peripherals::PIO0, 2>,
    _sm3: embassy_rp::pio::StateMachine<'a, embassy_rp::peripherals::PIO0, 3>,
}

pub struct WifiManager {
    pub control: cyw43::Control<'static>,
    pub stack: &'static Stack<'static>,  // Reference to static stack
    _pio_keepalive: PioKeepalive<'static>,
}

impl WifiManager {
    /// Initialize the CYW43 WiFi chip and return the network device and control.
    /// The caller must create the Stack from the net_device and then call `new()` to create WifiManager.
    pub async fn init_cyw43(
        pins: WifiPins,
        irqs: impl Binding<
            embassy_rp::interrupt::typelevel::PIO0_IRQ_0,
            InterruptHandler<embassy_rp::peripherals::PIO0>,
        >,
        power_mode: cyw43::PowerManagementMode,
        spawner: embassy_executor::Spawner,
    ) -> (cyw43::Control<'static>, cyw43::NetDriver<'static>, PioKeepalive<'static>) {
        // Create WiFi control pins from peripherals
        let pwr = Output::new(pins.pwr, embassy_rp::gpio::Level::Low);
        let cs = Output::new(pins.cs, embassy_rp::gpio::Level::High);

        // 1. Initialize CYW43 WiFi chip
        let mut pio = Pio::new(pins.pio, irqs);
        let spi = PioSpi::new(
            &mut pio.common,
            pio.sm0,
            DEFAULT_CLOCK_DIVIDER,
            pio.irq0,
            cs,
            pins.dio,
            pins.clk,
            pins.dma,
        );
        let pio_keepalive = PioKeepalive {
            _common: pio.common,
            _irq_flags: pio.irq_flags,
            _irq1: pio.irq1,
            _irq2: pio.irq2,
            _irq3: pio.irq3,
            _sm1: pio.sm1,
            _sm2: pio.sm2,
            _sm3: pio.sm3,
        };

        static STATE: StaticCell<cyw43::State> = StaticCell::new();
        let state = STATE.init(cyw43::State::new());
        let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, CYW43_FW).await;

        spawner.spawn(cyw43_runner_task(runner).expect("failed to spawn cyw43_runner_task"));

        control.init(CYW43_CLM).await;
        control.set_power_management(power_mode).await;

        (control, net_device, pio_keepalive)
    }

    /// Create a new WifiManager with the given control, stack reference, and pio_keepalive.
    pub fn new(
        control: cyw43::Control<'static>,
        stack: &'static Stack<'static>,
        pio_keepalive: PioKeepalive<'static>,
    ) -> Self {
        Self {
            control,
            stack,
            _pio_keepalive: pio_keepalive,
        }
    }

    pub async fn start_ap_wpa2(&mut self, ap_ssid: &str, ap_password: &str, channel: u8) {
        self.control
            .start_ap_wpa2(ap_ssid, ap_password, channel)
            .await;
    }

    pub async fn join_network(&mut self, wifi_ssid: &str, wifi_password: &str) {
        loop {
            match self
                .control
                .join(wifi_ssid, cyw43::JoinOptions::new(wifi_password.as_bytes()))
                .await
            {
                Ok(()) => break,
                Err(_) => {
                    defmt::warn!("WiFi join failed, retrying...");
                }
            }
        }
        self.stack.wait_link_up().await;
        self.stack.wait_config_up().await;
    }
}

#[embassy_executor::task]
async fn cyw43_runner_task(
    runner: cyw43::Runner<
        'static,
        Output<'static>,
        PioSpi<'static, embassy_rp::peripherals::PIO0, 0, embassy_rp::peripherals::DMA_CH0>,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_runner_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

// =============================================================================
// Packet I/O Functions
// =============================================================================

/// Parse variable length integer from buffer
fn parse_remaining_length(buffer: &[u8]) -> Result<(usize, usize), ProtocolError> {
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

/// Read complete MQTT packet from socket
async fn read_packet<'a>(
    socket: &mut TcpSocket<'a>,
    buffer: &mut [u8],
    buffer_len: &mut usize,
) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, ProtocolError> {
    // Ensure at least 2 bytes
    while *buffer_len < 2 {
        match socket.read(&mut buffer[*buffer_len..]).await {
            Ok(0) => return Ok(None), // EOF
            Ok(n) => *buffer_len += n,
            Err(_) => return Err(ProtocolError::IncompletePacket { available: *buffer_len }),
        }
    }

    // Parse remaining length
    let (remaining_len, var_len_bytes) = parse_remaining_length(&buffer[..*buffer_len])?;
    let total_len = 1 + var_len_bytes + remaining_len;

    // Ensure full packet is available
    while *buffer_len < total_len {
        match socket.read(&mut buffer[*buffer_len..]).await {
            Ok(0) => return Err(ProtocolError::IncompletePacket { available: *buffer_len }),
            Ok(n) => *buffer_len += n,
            Err(_) => return Err(ProtocolError::IncompletePacket { available: *buffer_len }),
        }
    }

    // Decode packet
    let packet = Packet::decode(&buffer[..total_len])?;

    // Shift remaining data (manual buffer management, no BytesMut)
    let remaining = *buffer_len - total_len;
    buffer.copy_within(total_len.., 0);
    *buffer_len = remaining;

    Ok(Some(packet))
}

/// Write packet to socket
async fn write_packet<'a>(
    socket: &mut TcpSocket<'a>,
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    buffer: &mut [u8],
) -> Result<(), ProtocolError> {
    let size = packet.encode(buffer)
        .map_err(|_| ProtocolError::BufferTooSmall { buffer_size: 0 })?;

    // Embassy TcpSocket doesn't have write_all, so write in a loop
    let mut written = 0;
    while written < size {
        match socket.write(&buffer[written..size]).await {
            Ok(0) => return Err(ProtocolError::IncompletePacket { available: written }),
            Ok(n) => written += n,
            Err(_) => return Err(ProtocolError::BufferTooSmall { buffer_size: 0 }),
        }
    }

    Ok(())
}

// =============================================================================
// Cleanup Task
// =============================================================================

#[embassy_executor::task]
async fn cleanup_task(broker: &'static BrokerMutex) {
    defmt::info!("Cleanup task started");

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(5)).await;

        let current_time = current_time_nanos();
        let mut broker = broker.lock().await;

        // Clean expired sessions
        let expired: heapless::Vec<u128, 4> = broker
            .get_expired_sessions(current_time)
            .into_iter()
            .flatten()
            .collect();

        for session_id in expired {
            defmt::info!("Cleaning up expired session {}", session_id);
            broker.remove_session(session_id);
        }

        // Clean disconnected sessions
        let disconnected: heapless::Vec<u128, 4> = broker
            .get_disconnected_sessions()
            .into_iter()
            .flatten()
            .collect();

        for session_id in disconnected {
            defmt::info!("Cleaning up disconnected session {}", session_id);
            broker.remove_session(session_id);
        }
    }
}

// =============================================================================
// Accept Task
// =============================================================================

#[embassy_executor::task(pool_size = MAX_SESSIONS)]
async fn accept_task(
    stack: &'static Stack<'static>,
    broker: &'static BrokerMutex,
    session_id_gen: &'static Mutex<ThreadModeRawMutex, SessionIdGen>,
    socket_idx: usize,
) {
    loop {
        // Create new socket (fixed pool pattern)
        let mut rx_buf = [0u8; RX_BUFFER_SIZE];
        let mut tx_buf = [0u8; TX_BUFFER_SIZE];
        let mut socket = TcpSocket::new(*stack, &mut rx_buf, &mut tx_buf);

        defmt::debug!("Socket {} waiting for connection on port {}", socket_idx, MQTT_PORT);

        if let Err(_) = socket.accept(MQTT_PORT).await {
            defmt::error!("Socket {} accept error", socket_idx);
            embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
            continue;
        }

        let session_id = session_id_gen.lock().await.generate();
        defmt::info!("Socket {} accepted session {}", socket_idx, session_id);

        // Register session with broker
        {
            let mut broker = broker.lock().await;
            if let Err(_) = broker.register_new_session(session_id, DEFAULT_KEEP_ALIVE, current_time_nanos()) {
                defmt::error!("Failed to register session {}", session_id);
                continue;
            }
        }

        // Static buffers for packet processing (local to this connection)
        let mut rx_buffer = [0u8; RX_BUFFER_SIZE];
        let mut tx_buffer = [0u8; TX_BUFFER_SIZE];
        let mut buffer_len = 0usize;
        let mut last_activity = embassy_time::Instant::now();
        let keep_alive_timeout = embassy_time::Duration::from_secs((DEFAULT_KEEP_ALIVE as u64 * 3 / 2) as u64);

        defmt::info!("Session {} entering main loop", session_id);

        // Handle connection inline (this blocks the accept task until disconnect)
        loop {
            // Calculate timeout duration
            let elapsed = last_activity.elapsed();
            let time_until_timeout = if elapsed > keep_alive_timeout {
                embassy_time::Duration::from_secs(0)
            } else {
                keep_alive_timeout - elapsed
            };

            // Check if timeout occurred before trying to read
            if elapsed >= keep_alive_timeout {
                defmt::info!("Session {} keep-alive timeout", session_id);
                break;
            }

            // Try to read with timeout using select
            let read_result = embassy_futures::select::select(
                socket.read(&mut rx_buffer[buffer_len..]),
                embassy_time::Timer::after(time_until_timeout),
            ).await;

            match read_result {
                embassy_futures::select::Either::First(result) => {
                    // Data received from socket
                    match result {
                        Ok(0) => {
                            // EOF - client closed connection
                            defmt::info!("Session {} client closed connection", session_id);
                            break;
                        }
                        Ok(n) => {
                            buffer_len += n;

                            // Try to read a complete packet
                            match read_packet(&mut socket, &mut rx_buffer, &mut buffer_len).await {
                                Ok(Some(packet)) => {
                                    last_activity = embassy_time::Instant::now();

                                    defmt::debug!("Session {} received packet type={}",
                                        session_id,
                                        match &packet {
                                            Packet::Connect(_) => "CONNECT",
                                            Packet::Publish(_) => "PUBLISH",
                                            Packet::Subscribe(_) => "SUBSCRIBE",
                                            Packet::PingReq(_) => "PINGREQ",
                                            Packet::Disconnect(_) => "DISCONNECT",
                                            _ => "OTHER",
                                        }
                                    );

                                    // Process with broker
                                    let mut broker = broker.lock().await;
                                    let current_time = current_time_nanos();

                                    let result = broker
                                        .queue_packet_received_from_client(session_id, packet, current_time)
                                        .and_then(|_| broker.process_all_session_packets());

                                    let tx_packets = match result {
                                        Ok(()) => {
                                            let mut packets = heapless::Vec::<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, 8>::new();
                                            while let Ok(Some(pkt)) = broker.dequeue_packet_to_send_to_client(session_id) {
                                                let _ = packets.push(pkt);
                                            }
                                            packets
                                        }
                                        Err(_) => {
                                            defmt::warn!("Error processing packet for session {}", session_id);
                                            let _ = broker.mark_session_disconnected(session_id);
                                            break;
                                        }
                                    };
                                    drop(broker);

                                    // Send packets to client
                                    for pkt in tx_packets {
                                        if let Err(_) = write_packet(&mut socket, &pkt, &mut tx_buffer).await {
                                            defmt::error!("Write error for session {}", session_id);
                                            break;
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Incomplete packet, continue reading
                                    continue;
                                }
                                Err(_) => {
                                    defmt::warn!("Protocol error for session {}", session_id);
                                    break;
                                }
                            }
                        }
                        Err(_) => {
                            defmt::warn!("Socket read error for session {}", session_id);
                            break;
                        }
                    }
                }
                embassy_futures::select::Either::Second(_) => {
                    // Timer fired - should timeout
                    defmt::info!("Session {} keep-alive timeout", session_id);
                    break;
                }
            }
        }

        // Cleanup
        defmt::info!("Session {} closing", session_id);
        let mut broker = broker.lock().await;
        broker.remove_session(session_id);

        defmt::info!("Socket {} ready for new connection", socket_idx);
    }
}





#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let pins = WifiPins {
        pio: p.PIO0,
        dma: p.DMA_CH0,
        clk: p.PIN_29,
        dio: p.PIN_24,
        cs: p.PIN_25,
        pwr: p.PIN_23,
    };

    // Use DHCP for STA mode
    let stack_config = Config::dhcpv4(Default::default());

    // Initialize CYW43 WiFi chip and get control + net_device
    let (control, net_device, pio_keepalive) = WifiManager::init_cyw43(
        pins,
        Irqs,
        cyw43::PowerManagementMode::PowerSave,
        spawner,
    ).await;

    // Create network stack and store in StaticCell
    let mut rng = embassy_rp::clocks::RoscRng;
    let seed = rng.next_u64();

    static RESOURCES: StaticCell<StackResources<6>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        stack_config,
        RESOURCES.init(StackResources::new()),
        seed,
    );

    // Store stack in StaticCell to get 'static reference
    let stack: &'static Stack<'static> = STACK_CELL.init(stack);

    // Spawn the network runner task
    spawner.spawn(net_runner_task(runner).expect("failed to spawn net_runner_task"));

    // Create WifiManager with the static stack reference
    let mut wifi_manager = WifiManager::new(control, stack, pio_keepalive);
    wifi_manager.join_network(WIFI_SSID, WIFI_PASSWORD).await;

    let ip_info = wifi_manager.stack.config_v4().unwrap();
    info!("WiFi connected successfully");
    info!("SSID: {}", WIFI_SSID);
    info!("IP Address: {}", ip_info.address);

    // Initialize MQTT broker and session ID generator, store references
    let broker: &'static BrokerMutex = BROKER_CELL.init(Mutex::new(MqttBroker::new()));
    let session_id_gen: &'static Mutex<ThreadModeRawMutex, SessionIdGen> =
        SESSION_ID_GEN_CELL.init(Mutex::new(SessionIdGen::new()));
    info!("MQTT broker initialized");

    // Spawn cleanup task with broker reference
    spawner.spawn(cleanup_task(broker).expect("failed to spawn cleanup task"));
    info!("Cleanup task spawned");

    // Spawn accept tasks for fixed socket pool with all references
    for idx in 0..MAX_SESSIONS {
        spawner.spawn(accept_task(stack, broker, session_id_gen, idx)
            .expect("failed to spawn accept task"));
    }
    info!("MQTT server listening on port 1883");
    info!("Supporting up to {} concurrent sessions", MAX_SESSIONS);

    // Keep executor alive
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}
