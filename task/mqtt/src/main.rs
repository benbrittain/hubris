// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use drv_nrf52_uart_api::Uart;
use drv_sensirion_sps32::{Sensiron, SensironError};
use heapless::Vec;
use minimq::Error as MqError;
use minimq::{self, Minimq, Property, QoS, Retain};
use postcard::{from_bytes, to_vec};
use task_aether_api::*;
use userlib::*;

mod clock_interop;
mod tcp_interop;

use clock_interop::ClockLayer;
use tcp_interop::NetworkLayer;

task_slot!(AETHER, aether);
task_slot!(UART, uart);

pub const AETHER_NOTIFICATION: u32 = 1 << 0;
pub const PARTICLE_TIMER_NOTIFICATION: u32 = 1 << 7;
pub const CO2_TIMER_NOTIFICATION: u32 = 1 << 8;
pub const PARTICLE_TIMER_INTERVAL: u64 = 1000;
pub const CO2_TIMER_INTERVAL: u64 = 1200;

static SYS_LOGGER: SysLogger = SysLogger;
pub struct SysLogger;

#[derive(Debug)]
enum Error {
    Sensiron(SensironError),
    Mqtt(MqError<AetherError>),
}

impl log::Log for SysLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        userlib::sys_log!("{} - {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

struct AirQuality {
    mqtt: Minimq<NetworkLayer, ClockLayer, 256, 16>,
    aether: Aether,
    sensirion: Sensiron,
}

impl AirQuality {
    pub fn new(
        mqtt: Minimq<NetworkLayer, ClockLayer, 256, 16>,
        aether: Aether,
        sensirion: Sensiron,
    ) -> Result<Self, Error> {
        // Setup partical count peripheral
        let started = sensirion.start_measurement();
        if let Err(err) = started {
            // The device could already be on
            if err != SensironError::WrongDeviceState {
                return Err(Error::Sensiron(err));
            }
        }

        Ok(AirQuality {
            mqtt,
            aether,
            sensirion,
        })
    }

    /// Publish the gas sensor data over mqtt if any is available
    fn send_gas_sensor_data(&mut self) {

    }

    /// Publish the particle sensor data over mqtt if any is available
    fn send_particle_data(&mut self) {
        if let Ok(Some(sensor_data)) = self.sensirion.read_value() {
            // this is literally the same structure, maybe
            // make the sps32 crate return this data directly?
            let sensor_data = air_quality_messages::Particles {
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
            self.mqtt
                .client
                .publish(
                    "particle",
                    encoded_msg.as_slice(),
                    QoS::AtMostOnce,
                    Retain::NotRetained,
                    //&[Property::UserProperty("version", "0")],
                    &[],
                )
                .unwrap();
            sys_log!("published");
        }
    }

    fn poll(&mut self) -> Result<(), Error> {
        self.send_particle_data();
        self.mqtt
            .poll(|client, topic, message, properties| {
                sys_log!("polling");
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
            .map_err(|e| Error::Mqtt(e))?;
        Ok(())
    }
}

#[export_name = "main"]
fn main() -> ! {
    log::set_logger(&SYS_LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);

    let uart = Uart::from(UART.get_task_id());
    let aether = Aether::from(AETHER.get_task_id());
    let addr = aether.resolve("portal.local".into()).unwrap();
    let mut mqtt: Minimq<_, _, 256, 16> = Minimq::new(
        addr.0.into(),
        "mqtt-aethereo",
        NetworkLayer {
            aether: aether.clone(),
            socket: SocketName::mqtt,
        },
        ClockLayer {},
    )
    .unwrap();

    let sensirion = Sensiron::new(uart);

    let mut aq = AirQuality::new(mqtt, aether, sensirion).unwrap();

    loop {
        aq.poll().unwrap();
    }
}

//
//    let mut subscribed = false;
//    sys_set_timer(Some(0), PARTICLE_TIMER_NOTIFICATION);
//
//    let mut msg = [0; 16];
//    loop {
//        sys_log!("LOOP'd : {}", sys_get_timer().now);
//        let msginfo = sys_recv_open(
//            &mut msg,
//            CO2_TIMER_NOTIFICATION
//                | PARTICLE_TIMER_NOTIFICATION
//                | AETHER_NOTIFICATION,
//        );
//        sys_log!("OP: {:?}", msginfo.operation);
//
//        if mqtt.client.is_connected() && !subscribed {
//            mqtt.client.subscribe("topic", &[]).unwrap();
//            subscribed = true;
//        }
//
//        if msginfo.operation & CO2_TIMER_NOTIFICATION != 0 {}
//
//        if msginfo.operation & PARTICLE_TIMER_NOTIFICATION != 0 {
//        }
//
//        mqtt.poll(|client, topic, message, properties| {
//            sys_log!("polling");
//            match topic {
//                "topic" => {
//                    let string = match core::str::from_utf8(message) {
//                        Ok(v) => v,
//                        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
//                    };
//                    sys_log!("mqtt> '{}': '{}'", topic, string);
//                    client
//                        .publish(
//                            "echo",
//                            message,
//                            QoS::AtMostOnce,
//                            Retain::NotRetained,
//                            &[],
//                        )
//                        .unwrap();
//                }
//                topic => sys_log!("Unknown topic: {}", topic),
//            };
//        })
//        .unwrap();
//
//        let target_time = sys_get_timer().now + PARTICLE_TIMER_INTERVAL;
//        sys_set_timer(Some(target_time), PARTICLE_TIMER_NOTIFICATION);
//        let target_time = sys_get_timer().now + CO2_TIMER_INTERVAL;
//        sys_set_timer(Some(target_time), CO2_TIMER_NOTIFICATION);
//    }
//}
