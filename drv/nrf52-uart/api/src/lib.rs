//! API crate for the nRF52840 UART

#![no_std]

// pub use drv_nrf52_gpio_common::*;
use userlib::*;

#[derive(Copy, Clone, Debug, FromPrimitive, PartialEq)]
pub enum UartError {
    Unknown,
}

impl From<u32> for UartError {
    fn from(_: u32) -> UartError {
        UartError::Unknown
    }
}

impl From<UartError> for u32 {
    fn from(rc: UartError) -> Self {
        rc as u32
    }
}

impl From<UartError> for u16 {
    fn from(rc: UartError) -> Self {
        rc as u16
    }
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
