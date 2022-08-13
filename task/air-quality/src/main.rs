// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! https://sensirion.com/media/documents/8600FF88/616542B5/Sensirion_PM_Sensors_Datasheet_SPS30.pdf

#![no_std]
#![no_main]

// use drv_nrf52_uart_api::Uart;
// use drv_sensirion_sps32::{Sensiron, SensironError};
use bme68x_rust::{
    Interface, CommInterface, Device, DeviceConfig, Error, Filter,
    GasHeaterConfig, Odr, OperationMode, Sample, SensorData,
};
use drv_spi_api as spi_api;
use userlib::*;

// task_slot!(UART, uart);
task_slot!(SPI, spi);

#[derive(Debug)]
struct NrfSpi {
    pub device_id: u8,
    pub spi: spi_api::Spi,
}

impl Interface for NrfSpi {
    fn interface_type(&self) -> CommInterface {
        CommInterface::SPI
    }
    fn read(&self, reg_addr: u8, reg_data: &mut [u8]) -> Result<(), bme68x_rust::Error> {
        sys_log!("> read");
        sys_log!("> read > spi.write");
        self.spi.write(self.device_id, &[reg_addr]);
        //sys_log!("> read > spi.read");
        //self.spi.read(self.device_id, reg_data);
//        self.spi.write(self.device_id, reg_data);
        Ok(())
//        Err(Error::Unknown)
        //todo!()
    }
    fn write(&self, _: u8, _: &[u8]) -> Result<(), bme68x_rust::Error> {
        sys_log!("> write GONNA FAIL");
        Err(Error::Unknown)
        //todo!()
    }
    fn delay(&self, _: u32) {
        userlib::hl::sleep_for(100)
        //todo!()
    }
}

#[export_name = "main"]
fn main() -> ! {
    let spi = spi_api::Spi::from(SPI.get_task_id());
    // let uart = Uart::from(UART.get_task_id());
    //let sensirion = Sensiron::new(uart);

    sys_log!("Hello from air-quality");
    let mut bme = match Device::initialize(NrfSpi{
        device_id: 0,
        spi,
    }) {
        Err(e) => {
            sys_log!("Hello from air-quality {:?}", e);
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
    )
    .unwrap();

    //
    //    let started = sensirion.start_measurement();
    //    if let Err(err) = started {
    //        // The device could already be on
    //        if err != SensironError::WrongDeviceState {
    //            panic!("Somthing is busted with the sensor!");
    //        }
    //    }
    //    hl::sleep_for(100);

    // configure heater
    bme.set_gas_heater_conf(
        OperationMode::Forced,
        GasHeaterConfig::default()
            .enable()
            .heater_temp(300)
            .heater_duration(100),
    ).unwrap();

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
