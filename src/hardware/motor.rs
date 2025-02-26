use embassy_rp::{
    gpio::Output,
    pwm::{ChannelAPin, Config, Pwm, SetDutyCycle, Slice},
    Peripheral,
};

pub struct Motor<'a, PWM: SetDutyCycle> {
    ina: Output<'a>,
    inb: Output<'a>,
    pwm: PWM,
}

impl<'a, PWM: SetDutyCycle> Motor<'a, PWM> {
    pub fn new(
        ina: impl Into<Output<'a>>,
        inb: impl Into<Output<'a>>,
        pwm: PWM
    ) -> Motor<'a, PWM> {
        Motor {
            ina: ina.into(),
            inb: inb.into(),
            pwm: pwm,
        }
    }
}

impl<'a, PWM: SetDutyCycle> Motor<'a, PWM> {
    pub fn clockwise(&mut self) {
        self.ina.set_high();
        self.inb.set_low();
    }

    pub fn counter_clockwise(&mut self) {
        self.ina.set_low();
        self.inb.set_high();
    }

    pub fn brake(&mut self) {
        self.ina.set_low();
        self.inb.set_low();
    }

    /// Sets speed in percent
    pub fn set_speed(&mut self, speed: u8) -> Result<(), PWM::Error>{
        self.pwm.set_duty_cycle_percent(speed)?;
        Ok(())
    }

    /// Returns the maximum
    pub fn get_max_duty(&self) -> u16 {
        self.pwm.max_duty_cycle()
    }

    /// Changes the motor speed
    pub fn set_duty(&mut self, duty: u16) -> Result<(), PWM::Error> {
        self.pwm.set_duty_cycle(duty)?;
        Ok(())
    }
}
