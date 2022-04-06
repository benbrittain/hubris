//! A driver for the NRF52840 GPIO

#![no_std]
#![no_main]

use drv_nrf52_gpio_api::{GpioError, Mode, OutputType, Pin, Port, Pull};
use idol_runtime::RequestError;
use nrf52840_pac as device;
use userlib::*;

struct GpioServer<'a> {
    p0: &'a device::p0::RegisterBlock,
    p1: &'a device::p0::RegisterBlock,
}

impl idl::InOrderGpioImpl for GpioServer<'_> {
    fn configure(
        &mut self,
        _msg: &RecvMessage,
        port: Port,
        pin: Pin,
        mode: Mode,
        output_type: OutputType,
        pull: Pull,
    ) -> Result<(), RequestError<GpioError>> {
        let port = match port {
            Port(0) => self.p0,
            Port(1) => self.p1,
            _ => panic!("Invalid port"),
        };

        let dir = match mode {
            Mode::Input | Mode::DisconnectedInput => {
                device::p0::pin_cnf::DIR_A::INPUT
            }
            Mode::Output => device::p0::pin_cnf::DIR_A::OUTPUT,
        };

        let input = match mode {
            Mode::Input => device::p0::pin_cnf::INPUT_A::CONNECT,
            Mode::Output | Mode::DisconnectedInput => {
                device::p0::pin_cnf::INPUT_A::DISCONNECT
            }
        };

        let pull = match pull {
            Pull::None => device::p0::pin_cnf::PULL_A::DISABLED,
            Pull::Up => device::p0::pin_cnf::PULL_A::PULLUP,
            Pull::Down => device::p0::pin_cnf::PULL_A::PULLDOWN,
        };

        let drive = match output_type {
            OutputType::PushPull => device::p0::pin_cnf::DRIVE_A::S0S1,
            OutputType::OpenDrain => device::p0::pin_cnf::DRIVE_A::S0D1,
        };

        port.pin_cnf[pin.0 as usize].write(|w| {
            w.dir()
                .variant(dir)
                .input()
                .variant(input)
                .pull()
                .variant(pull)
                .drive()
                .variant(drive)
            //                .sense()
            //                .disabled()
        });

        Ok(())
    }

    fn toggle(
        &mut self,
        _msg: &RecvMessage,
        port: Port,
        pin: Pin,
    ) -> Result<(), RequestError<GpioError>> {
        assert!(pin.0 <= 31);
        let port = match port {
            Port(0) => self.p0,
            Port(1) => self.p1,
            _ => panic!("Invalid port"),
        };

        let pin_state = port.out.read().bits();
        let new_state = pin_state ^ (1 << pin.0 as u32);
        port.out.write(|w| unsafe { w.bits(new_state) });

        Ok(())
    }

    fn set_high(
        &mut self,
        _msg: &RecvMessage,
        port: Port,
        pin: Pin,
    ) -> Result<(), RequestError<GpioError>> {
        assert!(pin.0 <= 31);
        let port = match port {
            Port(0) => self.p0,
            Port(1) => self.p1,
            _ => panic!("Invalid port"),
        };

        let new_state = 1 << pin.0 as u32;
        port.out.write(|w| unsafe { w.bits(new_state) });

        Ok(())
    }
}

#[export_name = "main"]
fn main() -> ! {
    let p0 = unsafe { &*device::P0::ptr() };
    let p1 = unsafe { &*device::P1::ptr() };

    // TODO any setup we want to do

    // Field messages.
    let mut buffer = [0u8; idl::INCOMING_SIZE];
    let mut server = GpioServer { p0, p1 };
    loop {
        idol_runtime::dispatch(&mut buffer, &mut server);
    }
}

mod idl {
    use super::{GpioError, Mode, OutputType, Pin, Port, Pull};
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
