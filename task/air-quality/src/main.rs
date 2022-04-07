// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! https://sensirion.com/media/documents/8600FF88/616542B5/Sensirion_PM_Sensors_Datasheet_SPS30.pdf

#![no_std]
#![no_main]

use drv_nrf52_uart_api::Uart;
use drv_sensirion_sps32::{Sensiron, SensironError};
use userlib::*;

task_slot!(UART, uart);

#[export_name = "main"]
fn main() -> ! {
    let uart = Uart::from(UART.get_task_id());
    let sensirion = Sensiron::new(uart);

    sys_log!("Hello from air-quality");

    let started = sensirion.start_measurement();
    if let Err(err) = started {
        // The device could already be on
        if err != SensironError::WrongDeviceState {
            panic!("Somthing is busted with the sensor!");
        }
    }
    hl::sleep_for(100);

    loop {
        if let Ok(Some(sensor_data)) = sensirion.read_value() {
            sys_log!("{:#?}", sensor_data);
        }
        hl::sleep_for(8000);
    }
}
