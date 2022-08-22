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
}

fn set_read_buf(twim: &Twim, buf: userlib::hl::Borrow) {
    unsafe {
        let len = buf.info().unwrap().len;
        twim.rxd.maxcnt.write(|w| w.maxcnt().bits(len as u16));
        twim.rxd
            .ptr
            .write(|w| w.ptr().bits(RX_BUFFER.as_ptr() as u32));
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
        sys_log!("RX {} | {:x?}", len, &RX_BUFFER[..len]);
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
        sys_log!("TX {} | {:x?}", len, &TX_BUFFER[..len]);
    }
}

#[export_name = "main"]
fn main() -> ! {
    ringbuf_entry!(Trace::Started);
    let gpio = Gpio::from(GPIO.get_task_id());

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

    // TODO the hubris i2c controller stuff is very stm32 specific
    // right now, punt on this and hardcode.
    //
    // let controllers = i2c_config::controllers();
    // let pins = i2c_config::pins();
    // let muxes = i2c_config::muxes();
    //    self.registers.enable.write(ENABLE::ENABLE::EnableMaster);

    let twim = unsafe { &*device::TWIM0::ptr() };

    twim.frequency.write(|w| w.frequency().k100());
    twim.enable.write(|w| w.enable().enabled());
    unsafe {
        twim.psel
            .scl
            .write(|w| w.port().bit(false).pin().bits(SCL_PIN));
        twim.psel
            .sda
            .write(|w| w.port().bit(false).pin().bits(SDA_PIN));
    }
    //sys_log!("{:x?}", twim.psel.scl.read().bits());

    //twim.enable.write(|w| w.enable().disabled());
    //twim.shorts.write(|w| w.lasttx_startrx().enabled().lastrx_stop().enabled());
    //sys_log!("{:x?}", twim.psel.scl.read().bits());
    //sys_log!("{:x?}", twim.psel.sda.read().bits());
    //sys_log!("{:x?}", twim.psel.sda.read().bits());

    sys_irq_control(1, true);
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
                sys_log!("I2C NOTIFICATION!");
                if bits & 1 != 0 {
                    if twim.events_txstarted.read().bits() == 1 {
                        ringbuf_entry!(Trace::TxStarted);
                        twim.events_txstarted.reset()
                    }
                    if twim.events_suspended.read().bits() == 1 {
                        ringbuf_entry!(Trace::Suspended);
                        twim.events_suspended.reset()
                    }
                    if twim.events_rxstarted.read().bits() == 1 {
                        ringbuf_entry!(Trace::RxStarted);
                        twim.events_rxstarted.reset()
                    }

                    if twim.events_lasttx.read().bits() == 1 {
                        ringbuf_entry!(Trace::LastTx);
                        twim.events_lasttx.reset()
                    }

                    if twim.events_lastrx.read().bits() == 1 {
                        ringbuf_entry!(Trace::LastRx);
                        twim.events_lastrx.reset()
                    }
                    if twim.events_error.read().bits() == 1 {
                        ringbuf_entry!(Trace::Error);
                        twim.events_error.reset();
                    }
                    if twim.events_stopped.read().bits() == 1 {
                        ringbuf_entry!(Trace::Stopped);
                        twim.events_stopped.reset()
                    }

                    sys_irq_control(1, true);
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
                    set_write_buf(twim, wbuf);
                    set_read_buf(twim, rbuf);

                    if winfo.len == 0 && rinfo.len == 0 {
                        return Err(ResponseCode::BadArg);
                    }

                    if winfo.len > TX_BUFFER_SIZE || rinfo.len > RX_BUFFER_SIZE
                    {
                        return Err(ResponseCode::BadArg);
                    }

                    // store the caller so we can respond when the interrupt fires

                    // set i2c device address
                    // _p_twim->ADDRESS = txAddress;
                    twim.address.write(|w| unsafe { w.address().bits(0x77) });

                    twim.events_stopped.write(|w| w.events_stopped().bit(false));
                    twim.events_txstarted.write(|w| w.events_txstarted().bit(false));
                    twim.tasks_resume.write(|w| w.tasks_resume().bit(true));
                    twim.tasks_starttx.write(|w| w.tasks_starttx().set_bit());

                    while twim.events_txstarted.read().bits() == 0 {
                        sys_log!("not done!");
                    }
                    twim.events_txstarted
                        .write(|w| w.events_txstarted().bit(false));
                    sys_log!("started");

                    while twim.events_lasttx.read().bits() == 0 {
                        //sys_log!("not done!");
                    }
                    twim.events_lasttx.write(|w| w.events_lasttx().bit(false));
                    sys_log!("last tx sent ");
                    //_p_twim->EVENTS_LASTTX = 0x0UL;
                    //sys_log!("tx started!");

                    //
                    *txref = Some(caller);

                    //twim.intenset.write(|w| w.txstarted().set_bit());
                    //twim.intenset.write(|w| w.lasttx().set_bit());
                    //twim.intenset.write(|w| w.stopped().set_bit());
                    //twim.intenset.write(|w| w.error().set_bit());

                    userlib::hl::sleep_for(100);

                    Ok(())
                }
                Op::WriteReadBlock => {
                    panic!("Don't handle this op");
                }
            },
        )
    }
}
