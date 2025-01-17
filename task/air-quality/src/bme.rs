use crate::bsec::bme::*;
use crate::bsec::Input;
use bme68x_rust::*;
use core::time::Duration;
use drv_i2c_api::{self as i2c_api, I2cDevice};
use userlib::*;

pub struct Bme {
    pub bme: Device<NrfI2c>,
}

impl Bme {
    pub fn initialize() -> Result<Self, Error> {
        let i2c = i2c_config::devices::bme68x(crate::I2C.get_task_id())[0];

        let mut bme = match Device::initialize(NrfI2c { i2c }) {
            Err(e) => {
                sys_log!("Error in air-quality {:?}", e);
                panic!();
            }
            Ok(b) => b,
        };

        // configure device
        bme.set_config(
            DeviceConfig::default()
                .filter(Filter::Off)
                .odr(Odr::StandbyNone)
                .oversample_humidity(Sample::X16)
                .oversample_pressure(Sample::Once)
                .oversample_temperature(Sample::X2),
        )?;

        // configure heater
        bme.set_gas_heater_conf(
            OperationMode::Forced,
            GasHeaterConfig::default()
                .enable()
                .heater_temp(300)
                .heater_duration(100),
        )?;

        Ok(Bme { bme })
    }
}

impl crate::bsec::bme::BmeSensor for Bme {
    type Error = bme68x_rust::Error;

    fn start_measurement(
        &mut self,
        settings: &BmeSettingsHandle,
    ) -> Result<Duration, Self::Error> {
        self.bme.set_config(
            DeviceConfig::default()
                .oversample_humidity(settings.humidity_oversampling())
                .oversample_pressure(settings.pressure_oversampling())
                .oversample_temperature(settings.temperature_oversampling()),
        )?;
        if settings.run_gas() {
            self.bme.set_gas_heater_conf(
                OperationMode::Forced,
                GasHeaterConfig::default()
                    .enable()
                    .heater_temp(settings.heater_temperature())
                    .heater_duration(settings.heating_duration()),
            );
        } else {
            self.bme.set_gas_heater_conf(
                OperationMode::Forced,
                GasHeaterConfig::default().disable(),
            );
        }
        let mut dur = self.bme.get_measure_duration(OperationMode::Forced);
        dur += settings.heating_duration() as u32;
        self.bme.set_op_mode(OperationMode::Forced).unwrap();
        Ok(Duration::from_nanos(dur as u64))
    }

    fn get_measurement(
        &mut self,
        inputs: &mut [Input],
    ) -> nb::Result<usize, Self::Error> {
        let data = self.bme.get_data(OperationMode::Forced).unwrap();
        use crate::bsec::InputKind::*;
        inputs[0].sensor_id = Temperature.into();
        inputs[0].signal = data.temperature;

        inputs[1].sensor_id = Pressure.into();
        inputs[1].signal = data.pressure;

        inputs[2].sensor_id = Humidity.into();
        inputs[2].signal = data.humidity;

        inputs[3].sensor_id = GasResistor.into();
        inputs[3].signal = data.gas_resistance;

        Ok(4)
    }
}

#[derive(Debug)]
pub struct NrfI2c {
    pub i2c: i2c_api::I2cDevice,
}

impl Interface for NrfI2c {
    fn interface_type(&self) -> CommInterface {
        CommInterface::I2C
    }

    fn read(
        &self,
        reg_addr: u8,
        reg_data: &mut [u8],
    ) -> Result<(), bme68x_rust::Error> {
        self.i2c
            .read_reg_into(reg_addr, reg_data)
            .map_err(|_| bme68x_rust::Error::Unknown)?;
        Ok(())
    }

    fn write(
        &self,
        reg_addr: u8,
        buf: &[u8],
    ) -> Result<(), bme68x_rust::Error> {
        let mut new_buf = [0; 16];
        new_buf[0] = reg_addr;
        new_buf[1..buf.len() + 1].copy_from_slice(buf);
        self.i2c
            .write(&new_buf[..buf.len() + 1])
            .map_err(|_| bme68x_rust::Error::Unknown)?;
        Ok(())
    }

    fn delay(&self, d: u32) {
        userlib::hl::sleep_for((d / 100).into())
    }
}

include!(concat!(env!("OUT_DIR"), "/i2c_config.rs"));
