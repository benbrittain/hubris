// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! https://sensirion.com/media/documents/8600FF88/616542B5/Sensirion_PM_Sensors_Datasheet_SPS30.pdf
//!
//! this library is absolutly miserable with stack space.
//!
//! move the sensirion_hdlc crate into driver and refactor some
//! then we can alse get rid of the arrayvec dependency.

#![no_std]

use arrayvec::{Array, ArrayVec};
use drv_nrf52_uart_api::*;
use sensirion_hdlc::{decode, encode, SpecialChars};
use userlib::*;

pub static CHARS: SpecialChars = SpecialChars::const_default();

enum Cmd {
    // Execute
    StartMeasurement = 0x0,
    // Execute
    StopMeasurement = 0x1,
    // Read
    ReadValue = 0x3,
    // Execute
    Sleep = 0x10,
    // Execute
    WakeUp = 0x11,
    // Execute
    StartFanClean = 0x56,
    // Read/Write
    ReadWriteCleanInterval = 0x80,
    // Read
    DeviceInfo = 0xD0,
    // Read
    DeviceVersion = 0xD1,
    // Read
    DeviceStatus = 0xD2,
    // Read
    Reset = 0xD3,
}

#[derive(Debug)]
pub struct VersionInfo {
    major: u8,
    minor: u8,
    hardware_revision: u8,
    shdlc_major: u8,
    shdlc_minor: u8,
}

#[derive(Debug)]
pub struct SensorData {
    /// Mass Concentration PM1.0 (μg/m^3)
    pm1_0_mass: f32,
    /// Mass Concentration PM2.5 (μg/m^3)
    pm2_5_mass: f32,
    /// Mass Concentration PM4.0 (μg/m^3)
    pm4_0_mass: f32,
    /// Mass Concentration PM10.0 (μg/m^3)
    pm10_0_mass: f32,

    /// Number Concentration PM0.5 (#/cm^3)
    pm0_5_number: f32,
    /// Number Concentration PM1.0 (#/cm^3)
    pm1_0_number: f32,
    /// Number Concentration PM2.5 (#/cm^3)
    pm2_5_number: f32,
    /// Number Concentration PM4.0 (#/cm^3)
    pm4_0_number: f32,
    /// Number Concentration PM10.0 (#/cm^3)
    pm10_0_number: f32,

    /// Typical Partical Size (μm)
    partical_size: f32,
}

pub struct Sensiron {
    uart: Uart,
}

#[derive(Debug, PartialEq)]
pub enum SensironError {
    /// Error from HDLC data framing step
    HdlcError(sensirion_hdlc::HDLCError),
    /// Error from the UART driver
    UartError(UartError),
    /// Response was not for the sent command
    MismatchedResponse,
    /// Command returned data when it shouldn't have
    UnknownDataResponse,
    /// Wrong data length for this command (too much or little data)
    WrongDataLength,
    /// Unknown command
    UnknownCommand,
    /// No access right for command
    AccessRight,
    /// Command not allowed in current state
    WrongDeviceState,
    /// Unknown Device State error code
    UnknownDeviceError,
}

/// Look at the data section of the unframed packet
fn view_data<'a>(bytes: &'a ArrayVec<[u8; 1024]>) -> &'a [u8] {
    let data_len = bytes[3];
    &bytes[4..(4 + data_len) as usize]
}

fn convert_to_float(bytes: &[u8]) -> f32 {
    assert!(bytes.len() == 4);
    let num: u32 = (bytes[0] as u32) << 24
        | (bytes[1] as u32) << 16
        | (bytes[2] as u32) << 8
        | bytes[3] as u32;
    f32::from_bits(num)
}

impl Sensiron {
    pub fn new(uart: Uart) -> Self {
        Sensiron { uart }
    }

    pub fn read_value(&self) -> Result<Option<SensorData>, SensironError> {
        let cmd = [0x00, Cmd::ReadValue as u8, 0x00];
        let resp = self.write_bytes(&cmd)?;
        let data = view_data(&resp);

        // If we polled too frequently, this is just empty
        // but that's ok.
        if data.len() == 0 {
            return Ok(None);
        }

        Ok(Some(SensorData {
            pm1_0_mass: convert_to_float(&data[0..4]),
            pm2_5_mass: convert_to_float(&data[4..8]),
            pm4_0_mass: convert_to_float(&data[8..12]),
            pm10_0_mass: convert_to_float(&data[12..16]),
            pm0_5_number: convert_to_float(&data[16..20]),
            pm1_0_number: convert_to_float(&data[20..24]),
            pm2_5_number: convert_to_float(&data[24..28]),
            pm4_0_number: convert_to_float(&data[28..32]),
            pm10_0_number: convert_to_float(&data[32..36]),
            partical_size: convert_to_float(&data[36..40]),
        }))
    }

    pub fn start_measurement(&self) -> Result<(), SensironError> {
        let cmd = [0x00, Cmd::StartMeasurement as u8, 0x02, 0x01, 0x03];

        let resp = self.write_bytes(&cmd)?;
        let data = view_data(&resp);

        // this never returns data
        if data.len() != 0 {
            return Err(SensironError::UnknownDataResponse);
        }

        Ok(())
    }

    pub fn read_version(&self) -> Result<VersionInfo, SensironError> {
        let cmd = [0x00, Cmd::DeviceVersion as u8, 0x00];

        let resp = self.write_bytes(&cmd)?;
        let data = view_data(&resp);

        Ok(VersionInfo {
            major: data[0],
            minor: data[1],
            hardware_revision: data[3],
            shdlc_major: data[5],
            shdlc_minor: data[6],
        })
    }

    /// Frames the payload and verifies the response came
    /// from the expected command.
    fn write_bytes(
        &self,
        bytes: &[u8],
    ) -> Result<ArrayVec<[u8; 1024]>, SensironError> {
        let req_cmd = bytes[1];
        let encoded =
            encode(&bytes, CHARS).map_err(SensironError::HdlcError)?;

        self.uart.write(&encoded);

        hl::sleep_for(400);

        let mut buffer = [0; 128];
        let amount_read = self
            .uart
            .read(0, &mut buffer)
            .map_err(SensironError::UartError)?;

        let mut decoded = decode(&buffer[0..amount_read], CHARS)
            .map_err(SensironError::HdlcError)?;
        let addr = decoded[0];
        let resp_cmd = decoded[1];
        if req_cmd != resp_cmd {
            return Err(SensironError::MismatchedResponse);
        }

        // check the device state bits for an error
        let device_state = decoded[2];
        match device_state {
            0x00 => Ok(decoded),
            0x01 => Err(SensironError::WrongDataLength),
            0x02 => Err(SensironError::UnknownCommand),
            0x03 => Err(SensironError::AccessRight),
            0x43 => Err(SensironError::WrongDeviceState),
            _ => Err(SensironError::UnknownDeviceError),
        }
    }
}
