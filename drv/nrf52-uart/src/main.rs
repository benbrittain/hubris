//!
//! UART w/ EasyDMA
//!
//! Copies `BUFFER_SIZE` bytes out of sending task and transfers
//! it using EasyDMA.

#![no_std]
#![no_main]

use drv_nrf52_gpio_api::{self as gpio, Gpio};
use drv_nrf52_uart_api::UartError;
use idol_runtime::{NotificationHandler, RequestError};
use nrf52840_pac::{self as device, uarte0};
use userlib::*;
use zerocopy::AsBytes;

task_slot!(GPIO, gpio);

const BUFFER_SIZE: usize = 32;
static mut TX_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

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
        buffer: idol_runtime::Leased<idol_runtime::W, [u8]>,
    ) {
    }

    fn write(
        &mut self,
        msginfo: &RecvMessage,
        buffer: idol_runtime::Leased<idol_runtime::R, [u8]>,
    ) {
        // We use the Pipelined impl, but for now we only support one write
        // action at a time
        if self.current_txn.is_some() {}
        // Setup the state for the current transmission
        self.current_txn = Some(Transmit {
            task: msginfo.sender,
            pos: 0,
            len: buffer.len(),
        });

        // Set interest in the ENDRX/ENDTX interrupts which indicate the buffer is no longer
        // being modified or read.
        //
        // TODO only set endtx() ?
        self.uarte
            .intenset
            .modify(|_r, w| w.endrx().set().endtx().set());

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
        if bits & UART_IRQ_MASK != 0 {
            if let Some(txn) = self.current_txn.as_mut() {
                if transmit_bytes(&self.uarte, txn) {
                    self.current_txn = None;
                    stop_write(self.uarte);
                }
            }
            sys_irq_control(UART_IRQ_MASK, true);
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
    uarte.events_endtx.reset();
    uarte.events_txstopped.reset();
    uarte.events_txstarted.reset();
}

fn transmit_bytes(
    uarte: &device::uarte0::RegisterBlock,
    tx: &mut Transmit,
) -> bool {
    let (rc, len) =
        unsafe { sys_borrow_read(tx.task, 0, tx.pos, &mut TX_BUFFER) };

    if rc != 0 {
        sys_reply(tx.task, UartError::BadArg as u32, &[]);
        true
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

        tx.pos += len;
        if tx.pos == tx.len {
            sys_reply(tx.task, UartError::Success as u32, &[]);
            true
        } else if tx.pos > tx.len {
            panic!("This should not be possible!!");
        } else {
            false
        }
    }
}

mod idl {
    use super::UartError;
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
