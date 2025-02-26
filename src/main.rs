#![no_std]
#![no_main]
#![cfg_attr(feature = "notci", feature(impl_trait_in_assoc_type))]

use core::net::Ipv4Addr;

use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use dotenv_proc::{dotenv, dotenv_option};
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_executor::Spawner;
use embassy_net::{Ipv4Cidr, StackResources};
use embassy_rp::{
    bind_interrupts, clocks::RoscRng, config, gpio::{Input, Level, Output}, i2c::I2c, peripherals::{I2C1, PIO0, USB}, pio::Pio, pwm::{Pwm, PwmOutput}, usb::Driver, watchdog::Watchdog
};
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use hardware::motor::Motor;
use heapless::Vec;
use log::*;
use panic_reset as _;
use rand_core::RngCore;
use static_cell::StaticCell;
use tasks::*;

mod hardware;
mod tasks;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});

#[cfg(feature = "notci")]
const WIFI_SSID: &str = dotenv!("WIFI_SSID");
#[cfg(not(feature = "notci"))]
const WIFI_SSID: &str = "test";
const WIFI_PWD: Option<&str> = dotenv_option!("WIFI_PWD");

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    spawner.must_spawn(watchdog(Watchdog::new(p.WATCHDOG)));
    spawner.must_spawn(logger(p.USB));
    info!("Begin logging");

    let mut rng = RoscRng;

    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");
    // let fw: [u8; 1] = [0];
    // let clm: [u8; 1] = [0];

    // Set up the PIO for communication with the cyw34
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    // Initialized cyw43
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, cyw43_runner) = cyw43::new(state, pwr, spi, fw).await;

    spawner.spawn(networking::cyw43_task(cyw43_runner)).unwrap();

    // Initialize the controller
    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    control.gpio_set(0, true).await;

    let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Addr::new(10, 0, 0, 21), 24),
        dns_servers: Vec::new(),
        gateway: Some(Ipv4Addr::new(10, 0, 0, 1)),
    });

    // Init the network stack
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let (stack, net_runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        rng.next_u64(),
    );
    // Begin the cyw43 communication and start the server
    spawner
        .spawn(networking::begin_hosting_task(
            spawner, net_runner, control, stack,
        ))
        .unwrap();

    // Setup state loops
    let i2c1 = I2c::new_async(p.I2C1, p.PIN_15, p.PIN_14, Irqs, Default::default());
    static I2C_BUS: StaticCell<state::I2c1Bus> = StaticCell::new();
    let i2c_bus = I2C_BUS.init(Mutex::new(i2c1));
    spawner.spawn(state::update_ec_state_task(i2c_bus)).unwrap();
    spawner.spawn(state::update_ph_state_task(i2c_bus)).unwrap();
    // TODO: MAKE SURE this is the CORRECT PIN
    spawner
        .spawn(state::update_water_lvl_state_task(Input::new(
            p.PIN_10,
            embassy_rp::gpio::Pull::Down,
        )))
        .unwrap();

}

#[embassy_executor::task]
async fn watchdog(mut watchdog: Watchdog) {
    // If 2 cycles are missed, watchdog will trigger
    watchdog.start(Duration::from_secs(4));
    loop {
        // Feed watchdog every 2 secs
        watchdog.feed();
        Timer::after_secs(2).await;
    }
}

// Works now!
#[embassy_executor::task]
async fn logger(usb: USB) {
    // Create the USB driver.
    let driver = Driver::new(usb, Irqs);
    embassy_usb_logger::run!(1024, LevelFilter::Info, driver);
}
