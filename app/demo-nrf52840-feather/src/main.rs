// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

//#[cfg(not(any(feature = "panic-itm", feature = "panic-semihosting")))]
#[cfg(not(any(feature = "panic-semihosting")))]
compile_error!(
    "Must have either feature panic-itm or panic-semihosting enabled"
);
// Panic behavior controlled by Cargo features:
#[cfg(feature = "panic-itm")]
extern crate panic_itm; // breakpoint on `rust_begin_unwind` to catch panics
#[cfg(feature = "panic-semihosting")]
extern crate panic_semihosting; // requires a debugger

use nrf52840_pac;

use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    // 64 MHz
    const CYCLES_PER_MS: u32 = 64_000;

    unsafe { kern::startup::start_kernel(CYCLES_PER_MS) }
}
