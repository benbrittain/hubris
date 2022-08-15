// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use byteorder::ByteOrder;
use hif::*;
use hubris_num_tasks::Task;
use ringbuf::*;
use userlib::*;

#[derive(Copy, Clone, PartialEq)]
enum Trace {
    Execute((usize, hif::Op)),
    Failure(Failure),
    Success,
    None,
}

ringbuf!(Trace, 64, Trace::None);

pub struct Buffer(u8);

//
// The order in this enum must match the order in the functions array that
// is passed to execute.
//
pub enum Functions {
    Sleep(u16, u32),
    Send((Task, u16, Buffer, usize), u32),
    #[cfg(feature = "rng")]
    Rng(usize, drv_rng_api::RngError),
}

pub(crate) static HIFFY_FUNCS: &[Function] = &[
    crate::common::sleep,
    crate::common::send,
    #[cfg(feature = "rng")]
    crate::common::rng_fill,
];

//
// This definition forces the compiler to emit the DWARF needed for debuggers
// to be able to know function indices, arguments and return values.
//
#[used]
static HIFFY_FUNCTIONS: Option<&Functions> = None;

pub(crate) fn trace_execute(offset: usize, op: hif::Op) {
    ringbuf_entry!(Trace::Execute((offset, op)));
}

pub(crate) fn trace_success() {
    ringbuf_entry!(Trace::Success);
}

pub(crate) fn trace_failure(f: hif::Failure) {
    ringbuf_entry!(Trace::Failure(f));
}
