//!
//! UART w/ EasyDMA
//!
//! Copies `BUFFER_SIZE` bytes out of sending task and transfers
//! it using EasyDMA.

#![no_std]
#![no_main]

use drv_nrf52_gpio_api::{self as gpio, Gpio};
use drv_nrf52_uart_api::UartError;
use idol_runtime::NotificationHandler;
use nrf52840_pac::{self as device, uarte0};
use userlib::*;

task_slot!(GPIO, gpio);

const TX_BUFFER_SIZE: usize = 16;
static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

const RX_BUFFER_SIZE: usize = 2048;
static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];
static mut RX_BUF_CNT: usize = 0;

static mut RX_LOC: [u8; 1] = [0; 1];

struct Transmit {
    task: TaskId,
    len: usize,
    pos: usize,
}

const UART_IRQ_MASK: u32 = 1;

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
    )
    .unwrap();

    gpio.configure(
        rx.0,
        rx.1,
        gpio::Mode::Input,
        gpio::OutputType::PushPull,
        gpio::Pull::None,
    )
    .unwrap();

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
    uarte.events_endtx.reset();
    uarte.events_txstarted.reset();

    // Point to RX buffer
    uarte
        .rxd
        .ptr
        .write(|w| unsafe { w.ptr().bits(RX_LOC.as_ptr() as u32) });

    // TODO make much more than 1!
    uarte.rxd.maxcnt.write(|w| unsafe { w.maxcnt().bits(1) });

    // Interrupt whenever rx buffer is FULL
    uarte.intenset.modify(|_r, w| w.endrx().set());
    // Start RX task.
    uarte.tasks_startrx.write(|w| unsafe { w.bits(1) });
}

struct UarteServer<'a> {
    uarte: &'a device::uarte0::RegisterBlock,
    current_txn: Option<Transmit>,
}

impl idl::PipelinedUartImpl for UarteServer<'_> {
    fn configure(&mut self, _msg: &RecvMessage) {
        panic!("Not yet implemented!");
    }

    fn read(
        &mut self,
        msginfo: &RecvMessage,
        offset_into_buffer: usize,
        _buffer: idol_runtime::Leased<idol_runtime::W, [u8]>,
    ) {
        unsafe {
            let buf_cnt = RX_BUF_CNT;
            // todo ring buffer this or something
            let (_rc, _) = sys_borrow_write(
                msginfo.sender,
                0,
                offset_into_buffer,
                &RX_BUFFER[0..RX_BUF_CNT],
            );
            RX_BUFFER = [0; RX_BUFFER_SIZE];
            RX_BUF_CNT = 0;
            sys_reply(
                msginfo.sender,
                UartError::Success as u32,
                zerocopy::AsBytes::as_bytes(&buf_cnt),
            );
        }
    }

    fn write(
        &mut self,
        msginfo: &RecvMessage,
        buffer: idol_runtime::Leased<idol_runtime::R, [u8]>,
    ) {
        // We use the Pipelined impl, but for now we only support one write
        // action at a time
        if self.current_txn.is_some() {
            sys_reply(msginfo.sender, UartError::Busy as u32, &[]);
        }

        // Setup the state for the current transmission
        self.current_txn = Some(Transmit {
            task: msginfo.sender,
            pos: 0,
            len: buffer.len(),
        });

        // Set interest in the TXSTARTED/ENDTX interrupts which indicate the buffer is no longer
        // being written and that the starting has occured
        self.uarte
            .intenset
            .modify(|_r, w| w.txstarted().set().endtx().set());

        // Zero out the maxcnt so we don't write things from the old buffer
        self.uarte
            .txd
            .maxcnt
            .write(|w| unsafe { w.maxcnt().bits(0) });

        // Start TX task.
        self.uarte.tasks_starttx.write(|w| unsafe { w.bits(1) });
    }
}

impl NotificationHandler for UarteServer<'_> {
    fn current_notification_mask(&self) -> u32 {
        UART_IRQ_MASK
    }

    fn handle_notification(&mut self, bits: u32) {
        // When a UART interrupt is recieved, send bytes
        // and rearm the irq
        sys_irq_control(UART_IRQ_MASK, true);

        if bits & UART_IRQ_MASK != 0 {
            // remove?
            self.uarte.events_txdrdy.reset();
            self.uarte.events_txstarted.reset();

            if self.uarte.events_endrx.read().events_endrx().bit() {
                unsafe {
                    // just drop anything over for now
                    // the uart and hubris drivers better.
                    if RX_BUF_CNT < RX_BUFFER.len() {
                        RX_BUFFER[RX_BUF_CNT] = RX_LOC[0];
                        RX_BUF_CNT += 1;
                    } else {
                        panic!("dropped bytes!");
                    }
                    self.uarte.events_endrx.reset();
                    self.uarte.tasks_startrx.write(|w| w.bits(1));
                }
            }

            // If the endtx is set, we can proceed with more bytes
            if self.uarte.events_endtx.read().events_endtx().bit() {
                if self.current_txn.is_some() {
                    self.uarte.events_endtx.reset();
                    transmit_bytes(&self.uarte, &mut self.current_txn);
                }
            }

            // UARTE has been stopped, return control to sending task
            if self.uarte.events_txstopped.read().events_txstopped().bit() {
                if let Some(tx) = &mut self.current_txn {
                    let task = tx.task;
                    self.current_txn = None;
                    self.uarte.events_txstopped.reset();
                    self.uarte.events_endtx.reset();
                    sys_reply(task, UartError::Success as u32, &[]);
                }
            }
        }
    }
}

#[export_name = "main"]
fn main() -> ! {
    let uarte = unsafe { &*device::UARTE0::ptr() };

    setup_uarte(
        uarte,
        uarte0::baudrate::BAUDRATE_A::BAUD115200,
        (gpio::Port(0), gpio::Pin(25)),
        (gpio::Port(0), gpio::Pin(24)),
    );

    let mut buffer = [0u8; idl::INCOMING_SIZE];
    let mut server = UarteServer {
        uarte,
        current_txn: None,
    };

    sys_irq_control(UART_IRQ_MASK, true);

    loop {
        idol_runtime::dispatch_n(&mut buffer, &mut server);
    }
}

fn stop_write(uarte: &uarte0::RegisterBlock) {
    uarte.tasks_stoptx.write(|w| unsafe { w.bits(1) });
    uarte.events_txdrdy.reset();
    uarte.events_txstarted.reset();
    // We do the actual reply to the sending task when the
    // txstopped event triggers
}

fn transmit_bytes(
    uarte: &device::uarte0::RegisterBlock,
    transmit: &mut Option<Transmit>,
) {
    if let Some(tx) = transmit {
        // If we've already written the last set of bytes
        // and increment the pos, stop writing
        if tx.pos == tx.len {
            return stop_write(uarte);
        } else if tx.pos > tx.len {
            sys_reply(tx.task, UartError::Unrecoverable as u32, &[]);
        }

        let (rc, len) =
            unsafe { sys_borrow_read(tx.task, 0, tx.pos, &mut TX_BUFFER) };

        if rc != 0 {
            sys_reply(tx.task, UartError::BadArg as u32, &[]);
        } else {
            // Point the txd ptr register at the tx_buffer
            uarte
                .txd
                .ptr
                .write(|w| unsafe { w.ptr().bits(TX_BUFFER.as_ptr() as u32) });

            // Max Count is set to the amount of data borrowed from the sending task.
            uarte
                .txd
                .maxcnt
                .write(|w| unsafe { w.maxcnt().bits(len as _) });

            // we updated the maxcnt, so retrigger the start task
            uarte.tasks_starttx.write(|w| unsafe { w.bits(1) });

            tx.pos += len;
        }
    }
}

mod idl {
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
