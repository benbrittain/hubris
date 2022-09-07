// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A driver for the Nrf52 i2c chip.
//!
//! # IPC protocol
//!
//! ## `write` (1)
//!
//! Sends the contents of lease #0. Returns when completed.
//!
//!
//! ## `read` (2)
//!
//! Reads the buffer into lease #0. Returns when completed

#![no_std]
#![no_main]

use core::sync::atomic::{compiler_fence, Ordering::SeqCst};
use device::twis1::psel::SDA;
use drv_i2c_api::*;
use drv_nrf52_gpio_api::{self as gpio_api, Gpio};
use nrf52840_pac as device;
use ringbuf::*;
use userlib::*;

const TX_BUFFER_SIZE: usize = 255;
static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

const RX_BUFFER_SIZE: usize = 255;
static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];

/// D22 is P0.12 (SDA)
const SDA_PIN: u8 = 12;
/// D23 is P0.11 (SCL)
const SCL_PIN: u8 = 11;

ringbuf!(Trace, 16, Trace::None);

task_slot!(GPIO, gpio);

type Twim = nrf52840_pac::twim0::RegisterBlock;

#[derive(PartialEq, Clone, Copy)]
enum Trace {
    None,
    Started,
    WriteRead,
    BadProblem,
    Notification,
    TxStarted,
    RxStarted,
    LastTx,
    LastRx,
    Suspended,
    Stopped,
    Error,
    ErrorOverrun,
    ErrorDnack,
    ErrorAnack,
}

fn set_read_buf(twim: &Twim, buf: userlib::hl::Borrow) {
    unsafe {
        let len = buf.info().unwrap().len + 1;
        twim.rxd.maxcnt.write(|w| w.maxcnt().bits(len as u16));
        // sys_log!("RX {} | ", len);
        twim.rxd
            .ptr
            .write(|w| w.ptr().bits(RX_BUFFER.as_ptr() as u32));
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

fn set_write_buf(twim: &Twim, buf: userlib::hl::Borrow) {
    unsafe {
        buf.read_fully_at(0, &mut TX_BUFFER);
    }
    unsafe {
        let len = buf.info().unwrap().len;
        twim.txd.maxcnt.write(|w| w.maxcnt().bits(len as u16));
        twim.txd
            .ptr
            .write(|w| w.ptr().bits(TX_BUFFER.as_ptr() as u32));
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
        // sys_log!("TX {} | {:x?}", len, &TX_BUFFER[..len]);
    }
}

#[export_name = "main"]
fn main() -> ! {
    ringbuf_entry!(Trace::Started);
    let gpio = Gpio::from(GPIO.get_task_id());

    let twim = unsafe { &*device::TWIM1::ptr() };
    twim.enable.write(|w| w.enable().disabled());
    gpio.configure(
        gpio_api::Port(0),
        gpio_api::Pin(SCL_PIN),
        gpio_api::Mode::Input,
        gpio_api::OutputType::OpenDrain,
        gpio_api::Pull::Up,
    )
    .unwrap();
    gpio.configure(
        gpio_api::Port(0),
        gpio_api::Pin(SDA_PIN),
        gpio_api::Mode::Input,
        gpio_api::OutputType::OpenDrain,
        gpio_api::Pull::Up,
    )
    .unwrap();

    // Select pins.
    twim.psel.scl.write(|w| {
        unsafe { w.bits(SCL_PIN.into()) };
        w.connect().connected()
    });
    twim.psel.sda.write(|w| {
        unsafe { w.bits(SDA_PIN.into()) };
        w.connect().connected()
    });

    // Enable TWIM instance.
    twim.enable.write(|w| w.enable().enabled());

    // Configure frequency.
    twim.frequency.write(|w| w.frequency().k100());

    // TODO the hubris i2c controller stuff is very stm32 specific
    // right now, punt on this and hardcode.
    //
    // let controllers = i2c_config::controllers();
    // let pins = i2c_config::pins();
    // let muxes = i2c_config::muxes();
    //    self.registers.enable.write(ENABLE::ENABLE::EnableMaster);

    //    w.connect().connected()
    //});

    //// Configure frequency.
    ////        twim.frequency.write(|w| w.frequency().variant(frequency));

    ////sys_log!("{:x?}", twim.psel.scl.read().bits());

    sys_irq_control(1, true);
    //sys_log!("{:x?}", twim.psel.scl.read().bits());
    //sys_log!("{:x?}", twim.psel.sda.read().bits());
    //sys_log!("{:x?}", twim.psel.sda.read().bits());
    //   twim.enable.write(|w| w.enable().enabled());

    let mut tx: Option<hl::Caller<usize>> = None;
    let mask = 1;
    let mut buffer = [0; 4];
    loop {
        hl::recv(
            // Buffer (none required)
            &mut buffer,
            // Notification mask
            mask,
            // State to pass through to whichever closure below gets run
            &mut tx,
            // Notification handler
            |txref, bits| {
                // sys_log!("I2C NOTIFICATION!");
                if bits & 1 != 0 {
                    sys_irq_control(1, true);
                    if twim.events_suspended.read().bits() == 1 {
                        ringbuf_entry!(Trace::Suspended);
                        twim.events_suspended.reset()
                    }

                    if twim.events_txstarted.read().bits() == 1 {
                        ringbuf_entry!(Trace::TxStarted);
                        twim.events_txstarted.reset()
                    }

                    if twim.events_lasttx.read().bits() == 1 {
                        ringbuf_entry!(Trace::LastTx);
                        twim.events_lasttx.reset()
                    }

                    if twim.events_rxstarted.read().bits() == 1 {
                        ringbuf_entry!(Trace::RxStarted);
                        twim.events_rxstarted.reset()
                    }

                    if twim.events_lastrx.read().bits() == 1 {
                        ringbuf_entry!(Trace::LastRx);
                        twim.events_lastrx.reset()
                    }

                    if twim.events_error.read().bits() == 1 {
                        ringbuf_entry!(Trace::Error);
                        twim.events_error.reset();
                        // twim.tasks_stop.write(|w| unsafe { w.bits(1) });
                        let errorsrc = twim.errorsrc.read();
                        twim.errorsrc.reset();
                        //twim.errorsrc.write(|w| {
                        //    w.anack()
                        //        .not_received()
                        //        .dnack()
                        //        .not_received()
                        //        .overrun()
                        //        .not_received()
                        //});
                        // sys_log!("errorsource = {:x?}", errorsrc.bits());
                        if errorsrc.overrun().bit_is_set() {
                            ringbuf_entry!(Trace::ErrorOverrun);
                            //return txref
                            //    .take()
                            //    .unwrap()
                            //    .reply_fail(ResponseCode::BadResponse);
                            //let txref = txref.take().unwrap();
                            //return txref.reply(0);
                        }
                        if errorsrc.anack().bit_is_set() {
                            ringbuf_entry!(Trace::ErrorAnack);
                            return txref
                                .take()
                                .unwrap()
                                .reply_fail(ResponseCode::NoDevice);
                        }
                        if errorsrc.dnack().bit_is_set() {
                            ringbuf_entry!(Trace::ErrorDnack);
                            return txref
                                .take()
                                .unwrap()
                                .reply_fail(ResponseCode::NoRegister);
                        }
                    }
                    if twim.events_stopped.read().bits() == 1 {
                        ringbuf_entry!(Trace::Stopped);
                        twim.events_stopped.reset();

                        let txref = txref.take().unwrap();
                        let rbuf = txref.borrow(1);
                        unsafe {
                            rbuf.write_fully_at(0, &mut RX_BUFFER);
                        }

                        txref.reply(0)
                    }
                }
            },
            // Message handler
            |txref, op, msg| match op {
                Op::WriteRead => {
                    ringbuf_entry!(Trace::WriteRead);
                    let (payload, caller) = msg
                        .fixed_with_leases::<[u8; 4], usize>(2)
                        .ok_or(ResponseCode::BadArg)?;
                    let (addr, controller, port, mux) =
                        Marshal::unmarshal(payload)?;

                    if txref.is_some() {
                        ringbuf_entry!(Trace::BadProblem);
                        return Err(ResponseCode::Dead);
                    }

                    let borrow = caller.borrow(0);
                    let info = borrow.info().ok_or(ResponseCode::BadArg)?;
                    if !info.attributes.contains(LeaseAttributes::READ) {
                        return Err(ResponseCode::BadArg);
                    }

                    let wbuf = caller.borrow(0);
                    let winfo = wbuf.info().ok_or(ResponseCode::BadArg)?;

                    if !winfo.attributes.contains(LeaseAttributes::READ) {
                        return Err(ResponseCode::BadArg);
                    }

                    let rbuf = caller.borrow(1);
                    let rinfo = rbuf.info().ok_or(ResponseCode::BadArg)?;

                    // TODO there are def unneeded compiler_fences
                    // clean up.
                    compiler_fence(SeqCst);
                    set_write_buf(twim, wbuf);
                    compiler_fence(SeqCst);
                    set_read_buf(twim, rbuf);
                    compiler_fence(SeqCst);
                    twim.shorts.write(|w| {
                        w.lasttx_startrx().enabled().lastrx_stop().enabled()
                    });

                    if winfo.len == 0 && rinfo.len == 0 {
                        return Err(ResponseCode::BadArg);
                    }

                    if winfo.len > TX_BUFFER_SIZE || rinfo.len > RX_BUFFER_SIZE
                    {
                        return Err(ResponseCode::BadArg);
                    }

                    // set i2c device address
                    // _p_twim->ADDRESS = txAddress;
                    twim.address.write(|w| unsafe { w.address().bits(addr) });

                    twim.intenset.write(|w| w.txstarted().set_bit());
                    twim.intenset.write(|w| w.lasttx().set_bit());
                    twim.intenset.write(|w| w.stopped().set_bit());
                    twim.intenset.write(|w| w.error().set_bit());

                    twim.tasks_starttx.write(|w| w.tasks_starttx().set_bit());

                    compiler_fence(SeqCst);

                    // store the caller so we can respond when the interrupt fires
                    *txref = Some(caller);

                    Ok(())
                }
                Op::WriteReadBlock => {
                    panic!("Don't handle this op");
                }
            },
        )
    }
}
