use core::result::Result::{self, *};
use core::str::{self, FromStr, Utf8Error};
use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;
use heapless::String;
use thiserror::Error;

#[derive(Debug)]
#[allow(unused)]
pub enum EzoCommand {
    Baud,
    Calibrate,
    //ExportCal,
    FactoryReset,
    Find,
    Info,
    I2c,
    //ImportCal,
    Led,
    Name,
    Sleep,
    Read,
    Status,
    TempCompensation,
    TempCompAndRead,
}

impl EzoCommand {
    pub fn to_byte_string(&self) -> &[u8] {
        match self {
            Self::Baud => b"Baud",
            Self::Calibrate => b"Cal",
            //Self::ExportCal => b"Export",
            Self::FactoryReset => b"Factory",
            Self::Find => b"Find",
            Self::Info => b"i",
            Self::I2c => b"I2C",
            //Self::ImportCal => b"Import",
            Self::Led => b"L",
            Self::Name => b"Name",
            Self::Read => b"R",
            Self::Sleep => b"Sleep",
            Self::Status => b"Status",
            Self::TempCompensation => b"T",
            Self::TempCompAndRead => b"RT",
        }
    }
    pub fn get_cmd_delay_ms(&self) -> Option<u32> {
        match self {
            Self::Led => Some(300),
            Self::Find => Some(300),
            Self::Read => Some(900),
            Self::Calibrate => todo!(),
            Self::TempCompensation => Some(300),
            Self::TempCompAndRead => Some(900),
            Self::Name => Some(300),
            Self::Info => Some(300),
            Self::Status => Some(300),
            Self::Sleep => None,
            Self::I2c => None,
            Self::FactoryReset => None,
            Self::Baud => None,
        }
    }
}

pub struct EzoBoard<I2C: I2c> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> EzoBoard<I2C> {
    pub fn new(i2c: I2C, address: u8) -> Self {
        EzoBoard { i2c, address }
    }

    pub async fn send_command(&mut self, command: EzoCommand) -> Result<(), EzoBoardError> {
        self.i2c
            .write(self.address, command.to_byte_string())
            .await
            .map_err(|_| EzoBoardError::I2c)?;
        Ok(())
    }

    pub async fn read_response(&mut self) -> Result<String<40>, EzoBoardError> {
        let mut buff: [u8; 40] = [0; 40];
        self.i2c
            .read(self.address, &mut buff)
            .await
            .map_err(|_| EzoBoardError::I2c)?;
        match &buff[0] {
            // OK
            1 => {
                let out = String::from_str(str::from_utf8(&buff[1..])?)
                    .map_err(|_| EzoBoardError::StringParseError)?;
                Ok(out)
            }
            // Request Syntax Error
            2 => Err(EzoBoardError::SyntaxError),
            // Delay too small
            254 => Err(EzoBoardError::NotReady),
            // No data to send
            255 => Err(EzoBoardError::NoData),
            // Unknown
            _ => Err(EzoBoardError::Unknown),
        }
    }

    pub async fn send_and_recieve(
        &mut self,
        command: EzoCommand,
    ) -> Result<String<40>, EzoBoardError> {
        let Some(command_delay) = command.get_cmd_delay_ms() else {
            return Err(EzoBoardError::NoResponsePossible);
        };
        self.send_command(command).await?;

        Timer::after_millis(command_delay as u64).await;

        self.read_response().await
    }
}

#[derive(Debug, Error)]
pub enum EzoBoardError {
    #[error("I2c error")]
    I2c,
    #[error("Utf8Error: {0}")]
    Utf8Error(#[from] Utf8Error),
    #[error("String Parse Error")]
    StringParseError,
    #[error("Device was not ready")]
    NotReady,
    #[error("No data was sent")]
    NoData,
    #[error("Command syntax error")]
    SyntaxError,
    #[error("Unknown error")]
    Unknown,
    #[error("No response is possible from this command")]
    NoResponsePossible,
}
