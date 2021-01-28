#![no_std]
#![feature(asm)]
#![feature(naked_functions)]

pub use lpc55_romapi::FlashStatus;
use num_traits::cast::FromPrimitive;

/// Write the buffer to the specified region number.
#[cfg(not(feature = "standalone"))]
#[inline(never)]
pub fn hypo_write_to_flash(region: u32, buf: &[u8]) -> FlashStatus {
    // Do we need a bounds check here as well as the secure world or is that
    // redundant?
    let result = unsafe {
        core::mem::transmute::<
            _,
            unsafe extern "C" fn(u32, *const u8, u32) -> u32,
        >(__bootloader_fn_table.write_to_flash)(
            region,
            buf.as_ptr(),
            buf.len() as u32,
        )
    };

    let result = match FlashStatus::from_u32(result) {
        Some(a) => a,
        None => FlashStatus::Unknown,
    };

    return result;
}

#[cfg(feature = "standalone")]
pub fn hypo_write_to_flash(_addr: u32, _buf: &[u8], _size: u32) -> FlashStatus {
    return FlashStatus::Success;
}
include!(concat!(env!("OUT_DIR"), "/hypo.rs"));
