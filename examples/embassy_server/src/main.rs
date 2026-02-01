#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::info;
use embassy_executor::Spawner;
use embassy_net::{Config, Ipv4Address, Ipv4Cidr, StaticConfigV4, Stack, StackResources};
use embassy_rp::gpio::Output;
use embassy_rp::interrupt::typelevel::Binding;
use embassy_rp::pio::{InterruptHandler, Pio};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

embassy_rp::bind_interrupts!(pub struct Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

const CYW43_FW: &[u8] = include_bytes!("./cyw43-firmware/43439A0.bin");
const CYW43_CLM: &[u8] = include_bytes!("./cyw43-firmware/43439A0_clm.bin");

const AP_CHANNEL: u8 = 1;
const AP_SSID: &str = env!("AP_SSID");
const AP_PASSWORD: &str = env!("AP_PASSWORD");
const _: () = assert!(
    AP_PASSWORD.len() >= 8,
    "AP_PASSWORD must be at least 8 characters"
);

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

pub struct WifiConfig {
    pub power_mode: cyw43::PowerManagementMode,
    pub stack_config: embassy_net::Config,
}

pub struct WifiManager {
    pub control: cyw43::Control<'static>,
    pub stack: Stack<'static>,
    _pio_keepalive: PioKeepalive<'static>,
}

impl WifiManager {
    pub async fn init_wifi(
        pins: WifiPins,
        irqs: impl Binding<
            embassy_rp::interrupt::typelevel::PIO0_IRQ_0,
            InterruptHandler<embassy_rp::peripherals::PIO0>,
        >,
        config: WifiConfig,
        spawner: embassy_executor::Spawner,
    ) -> WifiManager {
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
        control.set_power_management(config.power_mode).await;

        // 2. Initialize network stack
        let mut rng = embassy_rp::clocks::RoscRng;
        let seed = rng.next_u64();

        static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
        let (stack, runner) = embassy_net::new(
            net_device,
            config.stack_config,
            RESOURCES.init(StackResources::new()),
            seed,
        );

        // Spawn the network runner task
        spawner.spawn(net_runner_task(runner).expect("failed to spawn net_runner_task"));

        WifiManager { control, stack, _pio_keepalive: pio_keepalive }
    }

    pub async fn start_ap_wpa2(&mut self, ap_ssid: &str, ap_password: &str, channel: u8) {
        self.control
            .start_ap_wpa2(ap_ssid, ap_password, channel)
            .await;
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

    // CRITICAL: Use static IP for AP mode (no DHCP)
    let ip = Ipv4Address::new(169, 254, 1, 1);
    let config = WifiConfig {
        power_mode: cyw43::PowerManagementMode::None,
        stack_config: Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(ip.clone(), 16),
            gateway: None,
            dns_servers: Default::default(),
        }),
    };

    let mut wifi_manager = WifiManager::init_wifi(pins, Irqs, config, spawner).await;
    wifi_manager
        .start_ap_wpa2(AP_SSID, AP_PASSWORD, AP_CHANNEL)
        .await;

    info!("WiFi AP started successfully");
    info!("AP SSID: {}", AP_SSID);
    info!("AP Password: {}", AP_PASSWORD);
    info!("AP Gateway IP: {}", ip);

    // Keep executor alive
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}
