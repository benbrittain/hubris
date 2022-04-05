//!
//! UART w/ EasyDMA
//!
//! Copies `BUFFER_SIZE` bytes out of sending task and transfers
//! it using EasyDMA.

#![no_std]
#![no_main]

use drv_nrf52_gpio_api::{self as gpio, Gpio};
use nrf52840_pac::{self as device, uarte0};
use userlib::*;
use zerocopy::AsBytes;

task_slot!(GPIO, gpio);

const BUFFER_SIZE: usize = 32;
static mut tx_buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

#[repr(u32)]
enum ResponseCode {
    Success = 0,
    BadOp = 1,
    BadArg = 2,
    Busy = 3,
}

struct Transmit {
    task: TaskId,
    len: usize,
    pos: usize,
}

fn setup_uarte(
    uarte: &uarte0::RegisterBlock,
    baudrate: uarte0::baudrate::BAUDRATE_A,
    tx: (gpio::Port, gpio::Pin),
    rx: (gpio::Port, gpio::Pin),
) {
    // Setup GPIO pins.
    //
    // TODO(bwb) CTS/RTS pin configuration for flow control
    let gpio = Gpio::from(GPIO.get_task_id());
    gpio.configure(
        tx.0,
        tx.1,
        gpio::Mode::Output,
        gpio::OutputType::PushPull,
        gpio::Pull::None,
    );

    gpio.configure(
        rx.0,
        rx.1,
        gpio::Mode::Input,
        gpio::OutputType::PushPull,
        gpio::Pull::None,
    );

    // Configure UARTE to use those pins.
    uarte.psel.rxd.write(|w| {
        unsafe { w.bits(rx.1 .0 as u32) };
        w.connect().connected()
    });

    uarte.psel.txd.write(|w| {
        unsafe { w.bits(tx.1 .0 as u32) };
        w.connect().connected()
    });

    // Disable flow controly and exclude parity bit.
    //
    // TODO(bwb) make this configurable
    uarte
        .config
        .write(|w| w.hwfc().disabled().parity().excluded());

    // Set baud rate.
    uarte.baudrate.write(|w| w.baudrate().variant(baudrate));

    // Enable UARTE.
    uarte.enable.write(|w| w.enable().enabled());

    // Clear ENDTX event.
    uarte.events_endtx.write(|w| w.events_endtx().clear_bit());
}

#[export_name = "main"]
fn main() -> ! {
    let uarte = unsafe { &*device::UARTE0::ptr() };
    let mut nvic = unsafe { &*cortex_m::peripheral::NVIC::ptr() };

    setup_uarte(
        uarte,
        uarte0::baudrate::BAUDRATE_A::BAUD115200,
        (gpio::Port(0), gpio::Pin(25)),
        (gpio::Port(0), gpio::Pin(24)),
    );

    sys_irq_control(1, true);

    let mask = 1;
    let mut tx: Option<Transmit> = None;

    loop {
        let msginfo = sys_recv_open(&mut [], mask);
        if msginfo.sender == TaskId::KERNEL {
            if msginfo.operation & 1 != 0 {
                if let Some(txn) = tx.as_mut() {
                    if transmit_bytes(&uarte, txn) {
                        tx = None;
                        stop_write(uarte);
                    }
                }
                sys_irq_control(1, true);
            }
        } else {
            match msginfo.operation {
                OP_WRITE => {
                    // Deny incoming writes if we're already running one.
                    if tx.is_some() {
                        sys_reply(
                            msginfo.sender,
                            ResponseCode::Busy as u32,
                            &[],
                        );
                        continue;
                    }

                    // Check the lease count and characteristics.
                    if msginfo.lease_count != 1 {
                        sys_reply(
                            msginfo.sender,
                            ResponseCode::BadArg as u32,
                            &[],
                        );
                        continue;
                    }

                    let len = match sys_borrow_info(msginfo.sender, 0) {
                        None => {
                            sys_reply(
                                msginfo.sender,
                                ResponseCode::BadArg as u32,
                                &[],
                            );
                            continue;
                        }
                        Some(info)
                            if !info
                                .attributes
                                .contains(LeaseAttributes::READ) =>
                        {
                            sys_reply(
                                msginfo.sender,
                                ResponseCode::BadArg as u32,
                                &[],
                            );
                            continue;
                        }
                        Some(info) => info.len,
                    };

                    tx = Some(Transmit {
                        task: msginfo.sender,
                        pos: 0,
                        len,
                    });

                    // Set interest in the ENDRX/ENDTX interrupts which indicate the buffer is no longer
                    // being modified or read.
                    uarte
                        .intenset
                        .modify(|_r, w| w.endrx().set().endtx().set());

                    // Start TX task.
                    uarte.tasks_starttx.write(|w| unsafe { w.bits(1) });
                }
                _ => sys_reply(msginfo.sender, ResponseCode::BadOp as u32, &[]),
            }
        }
    }
}

fn stop_write(uarte: &uarte0::RegisterBlock) {
    uarte.tasks_stoptx.write(|w| unsafe { w.bits(1) });
    uarte.events_endtx.reset();
    uarte.events_txstopped.reset();
    uarte.events_txstarted.reset();
}

fn transmit_bytes(
    uarte: &device::uarte0::RegisterBlock,
    tx: &mut Transmit,
) -> bool {
    let (rc, len) =
        unsafe { sys_borrow_read(tx.task, 0, tx.pos, &mut tx_buffer) };

    if rc != 0 {
        sys_reply(tx.task, ResponseCode::BadArg as u32, &[]);
        true
    } else {
        // Point the txd ptr register at the tx_buffer
        uarte
            .txd
            .ptr
            .write(|w| unsafe { w.ptr().bits(tx_buffer.as_ptr() as u32) });

        // Max Count is set to the amount of data borrowed from the sending task.
        uarte
            .txd
            .maxcnt
            .write(|w| unsafe { w.maxcnt().bits(len as _) });

        tx.pos += len;

        if tx.pos == tx.len {
            sys_reply(tx.task, ResponseCode::Success as u32, &[]);
            true
        } else if tx.pos > tx.len {
            panic!("This should not be possible!!");
        } else {
            false
        }
    }
}
