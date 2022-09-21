// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use bme68x_rust::Interface;
use drv_nrf52_uart_api::Uart;
use drv_sensirion_sps32::{Sensiron, SensironError};
use heapless::Vec;
use minimq::Error as MqError;
use minimq::{self, Minimq, Property, QoS, Retain};
use postcard::{from_bytes, to_vec};
use serde::Serialize;
use task_aether_api::*;
use userlib::*;

use drv_i2c_api as i2c_api;

mod bme;
mod bsec;
mod clock_interop;
mod tcp_interop;

use bme::Bme;
use bsec::Bsec;
use clock_interop::ClockLayer;
use tcp_interop::NetworkLayer;

task_slot!(AETHER, aether);
task_slot!(UART, uart);
task_slot!(I2C, i2c_driver);

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
    Bsec(bsec::error::Error<bme68x_rust::Error>),
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
    bsec: Bsec<Bme>,
}

impl AirQuality {
    pub fn new(
        mqtt: Minimq<NetworkLayer, ClockLayer, 256, 16>,
        aether: Aether,
        sensirion: Sensiron,
        mut bsec: Bsec<Bme>,
    ) -> Result<Self, Error> {
        // Setup partical count peripheral
        let started = sensirion.start_measurement();
        if let Err(err) = started {
            // The device could already be on
            if err != SensironError::WrongDeviceState {
                return Err(Error::Sensiron(err));
            }
        }

        let mut sensors = Default::default();
        use bsec::{
            OutputKind, SampleRate, SubscriptionRequest, VirtualSensor,
        };
        bsec.update_subscription(
            &[
                SubscriptionRequest {
                    sample_rate: SampleRate::Lp.into(),
                    sensor_id: VirtualSensor::from(OutputKind::RawTemperature)
                        as u8,
                },
                SubscriptionRequest {
                    sample_rate: SampleRate::Lp.into(),
                    sensor_id: VirtualSensor::from(OutputKind::RawHumidity)
                        as u8,
                },
                SubscriptionRequest {
                    sample_rate: SampleRate::Lp.into(),
                    sensor_id: VirtualSensor::from(OutputKind::RawPressure)
                        as u8,
                },
                SubscriptionRequest {
                    sample_rate: SampleRate::Lp.into(),
                    sensor_id: VirtualSensor::from(OutputKind::Iaq) as u8,
                },
                SubscriptionRequest {
                    sample_rate: SampleRate::Lp.into(),
                    sensor_id: VirtualSensor::from(OutputKind::RawGas) as u8,
                },
            ],
            &mut sensors,
        )
        .map_err(|e| Error::Bsec(e))?;

        Ok(AirQuality {
            mqtt,
            aether,
            sensirion,
            bsec,
        })
    }

    /// Publish the gas sensor data over mqtt if any is available
    fn send_gas_sensor_data(&mut self) {
        let next_measurement = self.bsec.next_measurement();
        userlib::hl::sleep_until((next_measurement / 1_000_000) as u64);
        let dur = nb::block!(self.bsec.start_next_measurement()).unwrap();
        userlib::hl::sleep_for(dur.as_millis() as u64);
        let mut outputs: [_; 5] = Default::default();
        let num_out =
            nb::block!(self.bsec.process_last_measurement(&mut outputs))
                .unwrap();

        use crate::bsec::OutputKind;
        let mut humidity = None;
        let mut temperature = None;
        let mut pressure = None;
        let mut voc = None;
        let mut iaq = None;
        let mut iaq_accuracy = None;
        for sensor in &outputs[..num_out] {
            match OutputKind::try_from(sensor.sensor_id).unwrap() {
                OutputKind::Iaq => {
                    iaq = Some(sensor.signal);
                    iaq_accuracy = Some(sensor.accuracy);
                }
                OutputKind::RawGas => {
                    voc = Some(sensor.signal);
                }
                OutputKind::RawTemperature => {
                    temperature = Some(sensor.signal);
                }
                OutputKind::RawPressure => {
                    pressure = Some(sensor.signal);
                }
                OutputKind::RawHumidity => {
                    humidity = Some(sensor.signal);
                }
                _ => todo!(),
            }
        }

        let gas_data = air_quality_messages::Gases {
            humidity: humidity.expect("bad subscription"),
            temperature: temperature.expect("bad subscription"),
            pressure: pressure.expect("bad subscription"),
            iaq: iaq.expect("bad subscription"),
            iaq_accuracy: iaq_accuracy.expect("bad subscription"),
            voc: voc.expect("bad subscription"),
        };
        self.publish("gas", gas_data).unwrap();
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

            self.publish("particle", sensor_data).unwrap();
        }
    }

    fn publish<T: Serialize>(
        &mut self,
        channel: &str,
        msg: T,
    ) -> Result<(), Error> {
        let encoded_msg: Vec<u8, 128> = to_vec(&msg).unwrap();
        self.mqtt
            .client
            .publish(
                channel,
                encoded_msg.as_slice(),
                QoS::AtMostOnce,
                Retain::NotRetained,
                //&[Property::UserProperty("version", "0")],
                &[],
            )
            .map_err(|e| Error::Mqtt(e))
    }

    fn poll(&mut self) -> Result<(), Error> {
        self.send_particle_data();
        self.send_gas_sensor_data();
        self.mqtt
            .poll(|client, topic, message, properties| {
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

    let bme = Bme::initialize().unwrap();
    let mut bsec: Bsec<_> = bsec::Bsec::init(bme).unwrap();

    let mut aq = AirQuality::new(mqtt, aether, sensirion, bsec).unwrap();

    loop {
        if let Err(err) = aq.poll() {
            sys_log!("Error: {:?}", err);
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/i2c_config.rs"));
