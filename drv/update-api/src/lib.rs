// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]

use derive_idol_err::IdolError;
use userlib::{sys_send, FromPrimitive};
use zerocopy::{AsBytes, FromBytes};

#[derive(FromPrimitive, IdolError)]
#[repr(u32)]
pub enum UpdateError {
    BadHeader = 1,
    BadMagic = 2,
    BadLength = 3,
    UpdateInProgress = 4,
    BadBorrow = 5,
    OutOfBounds = 6,
    TooLong = 7,
    FlashWriteFail = 8,
    BadBlockSize = 9,
    Timeout = 10,
    BadResponse = 255,
}

pub const UPDATE_MAGIC: u32 = 0x1de0_4545;

#[derive(AsBytes, FromBytes, Copy, Clone, Default)]
#[repr(C)]
pub struct ImageHeader {
    pub magic: u32,
    pub byte_cnt: u32,
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
