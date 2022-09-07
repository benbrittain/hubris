// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! https://sensirion.com/media/documents/8600FF88/616542B5/Sensirion_PM_Sensors_Datasheet_SPS30.pdf

#![no_std]
#![no_main]

use bme68x_rust::{
    CommInterface, Device, DeviceConfig, Error, Filter, GasHeaterConfig,
    Interface, Odr, OperationMode, Sample, SensorData,
};
use drv_i2c_api as i2c_api;
use userlib::*;

task_slot!(I2C, i2c_driver);

include!(concat!(env!("OUT_DIR"), "/i2c_config.rs"));

#[derive(Debug)]
struct NrfI2c {
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
        let r = self.i2c.read_reg_into(reg_addr, reg_data);
        //sys_log!("erro: {:?}", r);
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
        //sys_log!("WRITE {:x}: {:x?}", reg_addr, buf);
        let r = self.i2c.write(&new_buf[..buf.len() + 1]);
        //sys_log!("res: {:?}", r);

        //sys_log!("WRITE 2");
        //self.i2c.write(buf);
        Ok(())
    }

    fn delay(&self, d: u32) {
        userlib::hl::sleep_for((d / 100).into())
    }
}

/// Set up the Bosch air quality peripheral
fn setup_bme(i2c: i2c_api::I2cDevice) -> Result<Device<NrfI2c>, Error> {
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

    Ok(bme)
}

#[export_name = "main"]
fn main() -> ! {
    let i2c = i2c_config::devices::bme68x(I2C.get_task_id())[0];
    let mut bme = setup_bme(i2c).unwrap();
    sys_log!("Hello from air-quality");

    sys_log!("TimeStamp(ms), Temperature(deg C), Pressure(Pa), Humidity(%%), Gas resistance(ohm)");
    loop {
        // Set operating mode
        bme.set_op_mode(OperationMode::Forced).unwrap();

        // Delay the remaining duration that can be used for heating
        let del_period =
            bme.get_measure_duration(OperationMode::Forced) + (300 * 1000);
        bme.interface.delay(del_period);

        // Get the sensor data
        if let Ok(data) = bme.get_data(OperationMode::Forced) {
            sys_log!(
                "{:?}, {:.2}, {:.2}, {:.2} {:.2}",
                sys_get_timer().now,
                data.temperature,
                data.pressure,
                data.humidity,
                data.gas_resistance,
            );
        }
    }
}
