//! API crate for the nRF52840 UART

#![no_std]

use userlib::*;

#[derive(Copy, Clone, Debug, FromPrimitive, PartialEq)]
pub enum UartError {
    Success,
    Busy,
    BadArg,
    Unrecoverable,
    Parity,
    Framing,
}

impl From<u32> for UartError {
    fn from(err: u32) -> UartError {
        UartError::from(err)
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
