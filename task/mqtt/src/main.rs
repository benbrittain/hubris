// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use drv_nrf52_uart_api::Uart;
use drv_sensirion_sps32::{Sensiron, SensironError};
use heapless::Vec;
use minimq::{self, Minimq, QoS, Retain};
use postcard::{from_bytes, to_vec};
use task_aether_api::*;
use userlib::*;

mod clock_interop;
mod tcp_interop;

use clock_interop::ClockLayer;
use tcp_interop::NetworkLayer;

task_slot!(AETHER, aether);
task_slot!(UART, uart);

#[export_name = "main"]
fn main() -> ! {
    let uart = Uart::from(UART.get_task_id());
    let aether = Aether::from(AETHER.get_task_id());
    let mut mqtt: Minimq<_, _, 256, 16> = Minimq::new(
        "fd00:1eaf::1".parse().unwrap(),
        "mqtt-aether",
        NetworkLayer {
            aether,
            socket: SocketName::mqtt,
        },
        ClockLayer {},
    )
    .unwrap();

    let mut subscribed = false;

    // Setup partical count peripheral
    let sensirion = Sensiron::new(uart);
    let started = sensirion.start_measurement();
    if let Err(err) = started {
        // The device could already be on
        if err != SensironError::WrongDeviceState {
            panic!("Somthing is busted with the sensor!");
        }
    }

    loop {
        if mqtt.client.is_connected() && !subscribed {
            mqtt.client.subscribe("topic", &[]).unwrap();
            subscribed = true;
        }

        if let Ok(Some(sensor_data)) = sensirion.read_value() {
            // this is literally the same structure, maybe
            // make the sps32 crate return this data directly?
            //
            // I didn't want the external dependency in it for
            // some reason.
            let sensor_data = air_quality_messages::SensorData {
                pm1_0_mass: sensor_data.pm1_0_mass,
                pm2_5_mass: sensor_data.pm2_5_mass,
                pm4_0_mass: sensor_data.pm4_0_mass,
                pm10_0_mass: sensor_data.pm10_0_mass,
                pm0_5_number: sensor_data.pm0_5_number,
                pm1_0_number: sensor_data.pm1_0_number,
                pm2_5_number: sensor_data.pm2_5_number,
                pm4_0_number: sensor_data.pm4_0_number,
                pm10_0_number: sensor_data.pm10_0_number,
                partical_size: sensor_data.partical_size,
            };
            let encoded_msg: Vec<u8, 128> = to_vec(&sensor_data).unwrap();
            mqtt.client
                .publish(
                    "particle",
                    encoded_msg.as_slice(),
                    QoS::AtMostOnce,
                    Retain::NotRetained,
                    &[],
                ).unwrap();
        }

        // TODO this only works when we have poll and that's kinda gross.
        // maybe write our own mqtt client?
        mqtt.poll(|client, topic, message, properties| {
            match topic {
                "topic" => {
                    let string = match core::str::from_utf8(message) {
                        Ok(v) => v,
                        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
                    };
                    sys_log!("mqtt> '{}': '{}'", topic, string);
                    client
                        .publish(
                            "echo",
                            message,
                            QoS::AtMostOnce,
                            Retain::NotRetained,
                            &[],
                        )
                        .unwrap();
                }
                topic => sys_log!("Unknown topic: {}", topic),
            };
        })
        .unwrap();
    }
}
