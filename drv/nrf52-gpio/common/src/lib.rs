#![no_std]

use userlib::FromPrimitive;
use zerocopy::AsBytes;

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, FromPrimitive, AsBytes)]
pub enum Mode {
    Input = 0b00,
    Output = 0b01,
    DisconnectedInput = 0b10,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, FromPrimitive, AsBytes)]
pub enum OutputType {
    PushPull = 0,
    OpenDrain = 1,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, FromPrimitive, AsBytes)]
pub enum Pull {
    /// Both resistors off.
    None = 0b00,
    /// Weak pull up.
    Up = 0b01,
    /// Weak pull down.
    Down = 0b10,
}
