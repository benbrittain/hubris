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
struct NrfSpi {
    pub i2c: i2c_api::I2cDevice,
}

impl Interface for NrfSpi {
    fn interface_type(&self) -> CommInterface {
        CommInterface::I2C
    }

    fn read(
        &self,
        reg_addr: u8,
        reg_data: &mut [u8],
    ) -> Result<(), bme68x_rust::Error> {
        todo!()
        //match self.device.read_reg::<u8, u8>(Register::ID as u8) {
        //    Ok(id) if id == ADT7420_ID => Ok(()),
        //    Ok(id) => Err(Error::BadID { id }),
        //    Err(code) => Err(Error::BadValidate { code }),
        //}
        //sys_log!("READ");
        //self.i2c.read();
        ////let resp = self.i2c.read();
        ////sys_log!("> read");
        ////sys_log!("> read > spi.write");
        ////self.spi.write(self.device_id, &[reg_addr]);
        ////sys_log!("> read > spi.read");
        ////self.spi.read(self.device_id, reg_data);
        //      //  self.spi.write(self.device_id, reg_data);
        //      //  Ok(())
        //Err(Error::Unknown)
        //todo!()
    }

    fn write(
        &self,
        reg_addr: u8,
        buf: &[u8],
    ) -> Result<(), bme68x_rust::Error> {
        sys_log!("WRITE 1");
        self.i2c.write(&[reg_addr]);
        sys_log!("WRITE 2");
        self.i2c.write(buf);
        Ok(())
    }

    fn delay(&self, _: u32) {
        userlib::hl::sleep_for(100)
        //todo!()
    }
}

#[export_name = "main"]
fn main() -> ! {
    let i2c_task = I2C.get_task_id();
    let i2c = i2c_config::devices::bme68x(I2C.get_task_id())[0];
    //let i2c = i2c_api::I2cDevice::from(I2C.get_task_id());

    userlib::hl::sleep_for(100);
    sys_log!("Hello from air-quality");
    sys_log!("reg write: {:?}", i2c.write(&[0xd0]));
   // sys_log!("reg write: {:?}", i2c.write(&[0xee, 0xd0]));
    sys_log!("reg read: {:?}", i2c.read_reg::<u8, u8>(0xd0));

    let mut bme = match Device::initialize(NrfSpi { i2c }) {
        Err(e) => {
            sys_log!("Hello from air-quality {:?}", e);
            panic!();
        }
        Ok(b) => b,
    };


    sys_log!("setting config");
    // configure device
    bme.set_config(
        DeviceConfig::default()
            .filter(Filter::Off)
            .odr(Odr::StandbyNone)
            .oversample_humidity(Sample::X16)
            .oversample_pressure(Sample::Once)
            .oversample_temperature(Sample::X2),
    )
    .unwrap();

    sys_log!("setting heater conf");
    // configure heater
    bme.set_gas_heater_conf(
        OperationMode::Forced,
        GasHeaterConfig::default()
            .enable()
            .heater_temp(300)
            .heater_duration(100),
    )
    .unwrap();

    //    let time_ms = core::time::Instant::now();
    sys_log!("Sample, TimeStamp(ms), Temperature(deg C), Pressure(Pa), Humidity(%%), Gas resistance(ohm), Status");
    for sample_count in 0..300 {
        // Set operating mode
        bme.set_op_mode(OperationMode::Forced).unwrap();

        // Delay the remaining duration that can be used for heating
        let del_period = bme
            .get_measure_duration(OperationMode::Forced)
            .wrapping_add(300 as u32 * 1000);
        bme.interface.delay(del_period);

        // Get the sensor data
        let mut n_fields = 0;
        let mut data: SensorData = SensorData::default();
        bme.get_data(1, &mut data, &mut n_fields).unwrap();

        if n_fields != 0 {
            sys_log!(
                "{}, {:?}, {:.2}, {:.2}, {:.2} {:.2} {:x}",
                sample_count,
                0,
                //time_ms.elapsed().as_millis(),
                data.temperature,
                data.pressure,
                data.humidity,
                data.gas_resistance,
                data.status,
            );
        }
    }

    loop {
        //        if let Ok(Some(sensor_data)) = sensirion.read_value() {
        //            sys_log!("{:#?}", sensor_data);
        //        }
        hl::sleep_for(8000);
    }
}
