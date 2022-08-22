// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]

use drv_nrf52_gpio_common::{Pin, Port};
use nrf52840_pac as device;
use userlib::sys_log;

const TX_BUFFER_SIZE: usize = 16;
static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

const RX_BUFFER_SIZE: usize = 16;
static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];

pub struct Spi {
    reg: &'static device::spim3::RegisterBlock,
}

impl From<&'static device::spim3::RegisterBlock> for Spi {
    fn from(reg: &'static device::spim3::RegisterBlock) -> Self {
        Self { reg }
    }
}

impl Spi {
    /// Initialize the SPI device to something reasonably. nRF wants us to fully configure
    /// peripherals before using them, so this function explicitly disables interrupts, and
    /// configures SPI mode 0 at the lowest frequency.
    pub fn initialize(&mut self) {
        self.reg.enable.write(|w| w.enable().disabled());
        self.reg.intenclr.write(|w| w.started().clear());
        self.reg.intenclr.write(|w| w.stopped().clear());
        self.configure_transmission_parameters(
            device::spim0::frequency::FREQUENCY_A::K125,
            device::spim0::config::ORDER_A::MSBFIRST,
            device::spim0::config::CPHA_A::LEADING,
            device::spim0::config::CPOL_A::ACTIVEHIGH,
        );
    }

    /// Reconfigure the SPI pinout.
    pub fn configure_pins(
        &mut self,
        miso_port: Port,
        miso_pin: Pin,
        mosi_port: Port,
        mosi_pin: Pin,
        sck_port: Port,
        sck_pin: Pin,
    ) {
        assert!(miso_pin.0 <= 31);
        assert!(mosi_pin.0 <= 31);
        assert!(sck_pin.0 <= 31);

        self.reg.psel.miso.write(|w| unsafe {
            w.port().bit(miso_port.0 == 1).pin().bits(miso_pin.0)
        });

        self.reg.psel.mosi.write(|w| unsafe {
            w.port().bit(mosi_port.0 == 1).pin().bits(mosi_pin.0)
        });

        self.reg.psel.sck.write(|w| unsafe {
            w.port().bit(sck_port.0 == 1).pin().bits(sck_pin.0)
        });

    }

    pub fn enable(
        &mut self,
    ) {
        self.reg.enable.write(|w| w.enable().enabled());
    }

    /// Disables the SPI device. Do not use it again without calling
    /// `reconfigure_and_enable`.
    pub fn disable(&mut self) {
        self.reg.enable.write(|w| w.enable().disabled());
    }

    /// Configure transmission parameters
    pub fn configure_transmission_parameters(
        &mut self,
        frequency: device::spim0::frequency::FREQUENCY_A,
        order: device::spim0::config::ORDER_A,
        cpha: device::spim0::config::CPHA_A,
        cpol: device::spim0::config::CPOL_A,
    ) {
        self.reg
            .frequency
            .write(|w| w.frequency().variant(frequency));

        self.reg.config.write(|w| {
            w
                .order().variant(order)
                .cpha().variant(cpha)
                .cpol().variant(cpol)
        });
    }

    /// Start a transaction. This just clears out the read buffer and the ready
    /// flag.
    pub fn start(&mut self) {
        sys_log!("start txbuffer: {:x?}", TX_BUFFER);
        self.reg.events_end.reset();
        self.reg.tasks_start.write(|w| w.tasks_start().set_bit());
        while !self.reg.events_end.read().events_end().bit_is_set() {}
        sys_log!("RX BUFFER AFTER EVENT_END: {:x?}", RX_BUFFER);
    }

    /// Checks if the ready flag is set. The ready flag is set whenever the SPI
    /// peripheral provides a new byte in the RXD read-register, and remains set
    /// until we clear it. recv8 clears this.
    pub fn is_read_ready(&self) -> bool {
        if self.reg.events_end.read().events_end().bit_is_set() {
            self.reg.events_end.reset();
            true
        } else {
            false
        }
    }

    /// Stuffs one byte of data into the SPI TX register.
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        unsafe {
            TX_BUFFER.clone_from_slice(&bytes[0..16]);
        }
        self.reg.txd.ptr.write(|w| unsafe { w.ptr().bits(TX_BUFFER.as_ptr() as u32) });
        self.reg.txd.maxcnt.write(|w| unsafe { w.maxcnt().bits(bytes.len() as u16) });
    }

    pub fn recv_bytes(&mut self, len: usize) {
        //unsafe {
        //    RX_BUFFER.clone_from_slice(&bytes[0..16]);
        //}
        self.reg.rxd.ptr.write(|w| unsafe { w.ptr().bits(RX_BUFFER.as_ptr() as u32) });
        self.reg.rxd.maxcnt.write(|w| unsafe { w.maxcnt().bits(len as u16) });
    }

    /// Pulls one byte of data from the SPI RX register. Also clears the
    /// ready event
    ///
    /// Preconditions:
    ///
    /// - There must be at least one byte of data in the receive register
    ///   (check `is_read_ready()`). Otherwise you'll just get some undefined data
    pub fn recv8(&mut self) -> u8 {
        todo!()
        // the spec sheet is not terribly clear on whether you have to
        // manually zero the events_ready register. I (artemis) experimented
        // with it and found that you do in fact need to do this.
        //self.reg.events_ready.write(|w| unsafe { w.bits(0) });
        //let b = self.reg.rxd.read().rxd().bits();
        //b
    }

    pub fn enable_transfer_interrupts(&mut self) {
        todo!()
        //self.reg.intenset.write(|w| w.ready().set());
    }

    pub fn disable_transfer_interrupts(&mut self) {
        //self.reg.intenclr.write(|w| w.ready().clear());
        todo!()
    }
}
