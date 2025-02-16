use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_rp::{
    gpio::Input,
    i2c::{Async, I2c},
    peripherals::I2C1,
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    mutex::Mutex,
};
use embassy_time::Timer;
use log::info;
use serde::{Deserialize, Serialize};

use crate::hardware::ezo::{EzoBoard, EzoCommand};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct HydroponicState {
    pub ec: EcState,
    pub ph: PhState,
    pub water_level: WaterLevelState,
}

impl HydroponicState {
    const fn initial_state() -> HydroponicState {
        HydroponicState {
            ec: EcState::Unknown,
            ph: PhState::Unknown,
            water_level: WaterLevelState::Unknown,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum EcState {
    #[default]
    Unknown,
    Good(f32),
    High(f32),
    Low(f32),
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Default)]
pub enum PhState {
    #[default]
    Unknown,
    Good(f32),
    High(f32),
    Low(f32),
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum WaterLevelState {
    #[default]
    Unknown,
    Good,
    Low,
}

const UPPER_LIMIT_PH: f32 = 7.4;
const LOWER_LIMIT_PH: f32 = 5.3;
const UPPER_LIMIT_EC: f32 = 1200.0;
const LOWER_LIMIT_EC: f32 = 1000.0;

pub type I2c1Bus = Mutex<NoopRawMutex, I2c<'static, I2C1, Async>>;

pub static MACHINE_STATE: Mutex<CriticalSectionRawMutex, HydroponicState> =
    Mutex::new(HydroponicState::initial_state());

#[embassy_executor::task]
pub async fn update_ec_state_task(i2c: &'static I2c1Bus) {
    // TODO SET TO CORRECT ADDR
    let mut ec_board = EzoBoard::new(I2cDevice::new(i2c), 0x20);

    loop {
        info!("Reading EC...");
        if let Ok(reading) = ec_board.send_and_recieve(EzoCommand::Read).await {
            let reading = reading.parse::<f32>().unwrap();
            if reading > UPPER_LIMIT_EC {
                MACHINE_STATE.lock().await.ec = EcState::High(reading);
            } else if reading < LOWER_LIMIT_EC {
                MACHINE_STATE.lock().await.ec = EcState::Low(reading);
            } else {
                MACHINE_STATE.lock().await.ec = EcState::Good(reading);
            }
        }

        // Waits 3 minutes before reading again
        Timer::after_secs(180).await;
    }
}

#[embassy_executor::task]
pub async fn update_ph_state_task(i2c: &'static I2c1Bus) {
    // TODO: SET TO CORRECT ADDR
    let mut ph_board = EzoBoard::new(I2cDevice::new(i2c), 0x21);

    loop {
        info!("Reading pH...");
        if let Ok(reading) = ph_board.send_and_recieve(EzoCommand::Read).await {
            let reading = reading.parse::<f32>().unwrap();
            if reading > UPPER_LIMIT_PH {
                MACHINE_STATE.lock().await.ph = PhState::High(reading);
            } else if reading < LOWER_LIMIT_PH {
                MACHINE_STATE.lock().await.ph = PhState::Low(reading);
            } else {
                MACHINE_STATE.lock().await.ph = PhState::Good(reading);
            }
        }

        // Waits 3 mins before reading again
        Timer::after_secs(180).await;
    }
}

#[embassy_executor::task]
pub async fn update_water_lvl_state_task(pin: Input<'static>) {
    loop {
        info!("Reading water level...");
        // Pin will be high is level is good
        if pin.is_high() {
            MACHINE_STATE.lock().await.water_level = WaterLevelState::Good;
        } else {
            MACHINE_STATE.lock().await.water_level = WaterLevelState::Low;
        }

        // Waits 10 minutes before reading again
        Timer::after_secs(600).await;
    }
}
