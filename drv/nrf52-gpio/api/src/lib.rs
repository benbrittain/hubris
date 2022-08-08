//! API crate for the nRF52840 GPIO

#![no_std]

pub use drv_nrf52_gpio_common::*;
use userlib::*;

#[derive(Copy, Clone, Debug, FromPrimitive, PartialEq)]
pub enum GpioError {
    Unknown,
}

impl From<u32> for GpioError {
    fn from(_: u32) -> GpioError {
        GpioError::Unknown
    }
}

impl From<GpioError> for u32 {
    fn from(rc: GpioError) -> Self {
        rc as u32
    }
}

impl From<GpioError> for u16 {
    fn from(rc: GpioError) -> Self {
        rc as u16
    }
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
