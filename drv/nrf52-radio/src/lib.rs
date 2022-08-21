//! Documentation for module found at
//! https://infocenter.nordicsemi.com/index.jsp?topic=%2Fps_nrf52840%2Fradio.html

#![no_std]

use core::{
    cell::{Cell, UnsafeCell},
    sync::atomic::{AtomicUsize, Ordering},
};
use device::radio::intenclr::DISABLED_A;
use nrf52840_pac as device;
use smoltcp::{
    phy::{Device, DeviceCapabilities, Medium},
    time::Instant,
    Result,
};
use task_aether_api::*;

mod phy;
mod ringbuf;

use crate::ringbuf::{RingBufferRx, RingBufferTx};

/// Mask of known bytes in ACK packet
pub const MHMU_MASK: u32 = 0xff000700;

#[derive(Debug, PartialEq, Copy, Clone)]
enum DriverState {
    /// Low power mode (disabled)
    Sleep,
    /// Before entering the sleep state,
    FallingAsleep,
    /// The receiver is enabled and it is receiving frames.
    Rx,
    /// The frame is received and the ACK is being transmitted.
    TxAck,
    /// Performing CCA followed by the frame transmission.
    CcaTx,
    /// Transmitting data frame (or beacon).
    Tx,
    /// Receiving ACK after the transmitted frame.
    RxAck,
    /// Performing the energy detection procedure.
    Ed,
    /// Performing the CCA procedure.
    Cca,
    /// Emitting the continuous carrier wave.
    ContinuousCarrier,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum RadioState {
    /// No operations are going on inside the radio and the power consumption is at a minimum.
    Disabled,
    /// The radio is ramping up and preparing for reception.
    RxRu,
    /// The radio is ready for reception to start.
    RxIdle,
    /// Reception has been started.
    Rx,
    /// The radio is disabling the receiver.
    RxDisable,
    /// The radio is ramping up and preparing for transmission.
    TxRu,
    /// The radio is ready for transmission to start.
    TxIdle,
    /// The radio is transmitting a packet.
    Tx,
    /// The radio is disabling the transmitter.
    TxDisable,
}

/// Interface to the radio peripheral.
pub struct Radio<'a> {
    radio: &'a device::radio::RegisterBlock,
    transmit_buffer: RingBufferTx<16>,
    receive_buffer: RingBufferRx<16>,
    mode: UnsafeCell<DriverState>,
    done_transmit_dbg: Cell<bool>,
}

impl Radio<'_> {
    pub fn new() -> Self {
        let radio = unsafe { &*device::RADIO::ptr() };

        Radio {
            radio,
            transmit_buffer: RingBufferTx::<16>::new(),
            receive_buffer: RingBufferRx::<16>::new(),
            mode: UnsafeCell::new(DriverState::Sleep),
            done_transmit_dbg: Cell::new(false),
        }
    }

    fn disable_interrupts(&self) {
        // TODO do this safely
        self.radio.intenclr.write(|w| unsafe { w.bits(0xffffffff) });
    }

    fn enable_interrupts(&self) {
        self.radio.intenset.write(|w| {
            w.ready()
                .set_bit()
                .ready()
                .set_bit()
                .ccaidle()
                .set_bit()
                .ccabusy()
                .set_bit()
                .end()
                .set_bit()
                .framestart()
                .set_bit()
        });
    }

    fn initialize_clocks(&self) {
        // HFCLK needs to be enabled
        let clock = unsafe { &*device::CLOCK::ptr() };
        clock
            .tasks_hfclkstart
            .write(|w| w.tasks_hfclkstart().set_bit());
        while clock
            .events_hfclkstarted
            .read()
            .events_hfclkstarted()
            .bit_is_clear()
        {
            // loop until started!
            cortex_m::asm::wfi();
        }
        clock.events_hfclkstarted.reset();

        // make sure we're using the external clock
        assert!(clock.hfclkstat.read().src().is_xtal());
    }

    /// Point the EasyDMA engine at a valid memory region
    /// for receptionand transmission of packets.
    fn configure_packet_buffer(&self, buf: &RingBufferTx<16>) {
        buf.set_as_buffer(&self);
    }

    /// Point the EasyDMA engine at a valid memory region
    /// for receptionand transmission of packets.
    fn configure_packet_buffer_recv(&self, buf: &RingBufferRx<16>) {
        buf.set_as_buffer(&self);
    }

    /// IEEE 802.15.4 implements a listen-before-talk channel access method
    /// to avoid collisions when transmitting.
    fn configure_cca(&self) {
        self.radio
            .ccactrl
            .write(|w| w.ccamode().carrier_and_ed_mode());
    }

    /// IEEE 802.15.4 uses a 16-bit ITU-T cyclic redundancy check (CRC)
    /// calculated over the MAC header (MHR) and MAC service data unit (MSDU).
    fn configure_crc(&self) {
        /// Polynomial used for CRC calculation in 802.15.4 frames
        pub const CRC_POLYNOMIAL: u32 = 0x011021;

        self.radio
            .crccnf
            .write(|w| w.skipaddr().ieee802154().len().two());
        self.radio
            .crcpoly
            .write(|w| unsafe { w.crcpoly().bits(CRC_POLYNOMIAL) });
        self.radio.crcinit.write(|w| unsafe { w.crcinit().bits(0) });
    }

    /// IEEE 802.15.4 uses packets of max 255 bytes with a 32bit
    /// all-zero preamble and crc included as part of the frame.
    fn configure_packets(&self) {
        self.radio.mode.write(|w| w.mode().ieee802154_250kbit());

        self.radio.pcnf1.write(|w| unsafe { w.maxlen().bits(127) });
        self.radio.pcnf0.write(|w| unsafe {
            w.lflen().bits(8).plen()._32bit_zero().crcinc().set_bit()
        });
    }

    pub fn initialize(&self) {
        // Setup high frequency clocks.
        self.initialize_clocks();

        // Disable any interrupts that might get sent during
        // initialization.
        self.disable_interrupts();

        // Turn on the radio peripheral.
        self.turn_on();

        // Configure radio for the assisted 802.15.4 mode.
        self.configure_packets();

        // Configure the radio for 802.15.4 CRC.
        self.configure_crc();

        // Configure the radio for 802.15.4 clear channel assessment.
        self.configure_cca();

        // Configure radio to use 2450Mhz aka Channel 20.
        // TODO don't hard code this.
        self.radio
            .frequency
            .write(|w| unsafe { w.frequency().bits(45) });

        // Configure radio to transmit at 4db
        // TODO don't hard code this.
        self.radio
            .txpower
            .write(|w| unsafe { w.txpower().bits(0x4) });

        // TODO explain this section
        self.radio
            .txaddress
            .write(|w| unsafe { w.txaddress().bits(0) });
        self.radio.rxaddresses.write(|w| w.addr0().set_bit());

        // enable fast ramp up
        self.radio.modecnf0.write(|w| w.ru().set_bit());

        // Start receiving...
        self.start_recv();
    }

    /// Check the state of the radio hardware.
    fn get_state(&self) -> RadioState {
        use device::radio::state::STATE_A::*;
        match self.radio.state.read().state().variant() {
            Some(TXIDLE) => RadioState::TxIdle,
            Some(RXIDLE) => RadioState::RxIdle,
            Some(RXRU) => RadioState::RxRu,
            Some(TXDISABLE) => RadioState::TxDisable,
            Some(RXDISABLE) => RadioState::RxDisable,
            Some(TX) => RadioState::Tx,
            Some(RX) => RadioState::Rx,
            Some(TXRU) => RadioState::TxRu,
            Some(DISABLED) => RadioState::Disabled,
            None => panic!("unknown radio state!"),
        }
    }

    pub fn can_recv(&mut self) -> bool {
        !self.receive_buffer.is_empty()
    }

    pub fn can_send(&mut self) -> bool {
        // notice this is the reverse of can_recv
        // we can only send from the perspective of smoltcp when the transmit
        // buffer is empty, but from smoltcp's perspective we need packets into
        // the recieve buffer to be conceptually capable of recieving.
        !self.transmit_buffer.is_full()
    }

    /// If we've gotten a packet, send it to smoltcp
    /// this does not read directly from the easyDMA buffer
    pub fn try_recv<R>(
        &self,
        read_buffer: impl FnOnce(&mut [u8]) -> R,
    ) -> Option<R> {
        self.receive_buffer.read(read_buffer)
    }

    /// Tries to send a packet, if TX buffer space is available.
    pub fn try_send<R>(
        &self,
        len: usize,
        build_packet: impl FnOnce(&mut [u8]) -> R,
    ) -> Option<R> {
        let resp = self.transmit_buffer.write(build_packet, len);
        self.start_transmit();
        resp
    }

    /// The radio and its registers will be reset to its initial
    /// state by switching the peripheral off.
    pub fn turn_off(&self) {
        self.set_mode(DriverState::Sleep);
        self.radio.power.write(|w| w.power().clear_bit());
        assert!(RadioState::Disabled == self.get_state());
    }

    /// After calling, the radio will be in it's default state
    /// with the radio disabled.
    pub fn turn_on(&self) {
        self.radio.power.write(|w| w.power().clear_bit());
        self.radio.power.write(|w| w.power().set_bit());

        // Make sure the radio is disabled so we start from a clean state.
        self.radio
            .tasks_disable
            .write(|w| w.tasks_disable().set_bit());
        while self
            .radio
            .events_disabled
            .read()
            .events_disabled()
            .bit_is_clear()
        {
            // loop until disabled!
            cortex_m::asm::wfi();
        }
        self.radio
            .events_disabled
            .write(|w| w.events_disabled().clear_bit());
    }

    /// Transition radio into transmit state
    pub fn start_transmit(&self) {
        self.turn_off();
        self.set_mode(DriverState::CcaTx);
        userlib::hl::sleep_for(40);

        self.configure_packet_buffer(&self.transmit_buffer);
        self.initialize();
    }

    /// Transition radio into receive state
    pub fn start_recv(&self) {
        self.radio.events_ready.reset();

        match self.get_driver_state() {
            // we always start recieving if starting from a sleep state
            DriverState::Sleep => {
                self.set_mode(DriverState::Rx);
                self.configure_packet_buffer_recv(&self.receive_buffer);
            }
            DriverState::Rx => {
                if self.get_state() == RadioState::RxIdle {
                    self.configure_packet_buffer_recv(&self.receive_buffer);
                    self.radio.tasks_start.write(|w| w.tasks_start().set_bit());
                    return;
                } else if self.get_state() == RadioState::TxIdle {
                    self.turn_off();
                    self.initialize();
                    self.set_mode(DriverState::Rx);
                    self.configure_packet_buffer_recv(&self.receive_buffer);
                    // don't do anything
                } else {
                    panic!(
                        "can't start from Rx driver state with command of {:?}",
                        self.get_state()
                    );
                }
            }
            DriverState::CcaTx => {
                self.configure_packet_buffer(&self.transmit_buffer);
            }
            state => panic!("Don't know how to start recv from {:?}", state),
        }

        self.enable_interrupts();
        self.radio.tasks_rxen.write(|w| w.tasks_rxen().set_bit());
    }

    fn set_mode(&self, mode: DriverState) {
        unsafe {
            let old_mode = (*self.mode.get());
            (*self.mode.get()) = mode;
        }
    }
    fn get_driver_state(&self) -> DriverState {
        unsafe { *self.mode.get() }
    }

    fn unknown_transition(&self, x: u32) {
        panic!(
            "ERROR@{} - Unknown transition for {:?} | {:?}",
            x,
            self.get_driver_state(),
            self.get_state()
        );
    }
    pub fn handle_interrupt(&mut self) {
        self.disable_interrupts();

        if self
            .radio
            .events_ccabusy
            .read()
            .events_ccabusy()
            .bit_is_set()
        {
            self.radio.events_ccabusy.reset();
        }

        if self
            .radio
            .events_ccaidle
            .read()
            .events_ccaidle()
            .bit_is_set()
        {
            self.radio.events_ccaidle.reset();
            self.set_mode(DriverState::Tx);
            self.radio.tasks_txen.write(|w| w.tasks_txen().set_bit());
        }

        if self.radio.events_ready.read().events_ready().bit_is_set() {
            // this should always be triggered in conjunction with
            // a tx/rx event ready state
            self.radio.events_ready.reset();
            // if not transmitting
            match self.get_driver_state() {
                DriverState::Rx => {
                    self.radio.tasks_start.write(|w| w.tasks_start().set_bit());
                }
                DriverState::Tx => {
                    assert!(self.get_state() == RadioState::TxIdle);
                    self.radio.tasks_start.write(|w| w.tasks_start().set_bit());
                }
                DriverState::CcaTx => {
                    self.radio
                        .tasks_ccastart
                        .write(|w| w.tasks_ccastart().set_bit());
                }
                state => self.unknown_transition(line!()),
            }
        }

        if self
            .radio
            .events_framestart
            .read()
            .events_framestart()
            .bit_is_set()
        {
            self.radio.events_framestart.reset();
        }

        if self.radio.events_end.read().events_end().bit_is_set() {
            self.radio.events_end.reset();

            match self.get_state() {
                RadioState::RxIdle => {
                    if self.radio.crcstatus.read().crcstatus().is_crcok() {
                        self.receive_buffer.got_packet();
                    } else {
                    }
                }
                RadioState::TxIdle => {
                    self.transmit_buffer.sent_packet();
                    // transition back to Rx
                    self.set_mode(DriverState::Rx);
                    if !self.transmit_buffer.is_empty() {
                        self.start_transmit();
                    }
                }
                s => {
                    panic!("Don't know how to handle {:?} during event end", s)
                }
            }

            // transition back to recv
            self.start_recv();
        }

        self.enable_interrupts();
    }

    /// Get the device specific IEEE 802.15.4 long/extended address
    pub fn get_addr(&mut self) -> Ieee802154Address {
        let ficr = unsafe { &*device::FICR::ptr() };

        let device_addr1: [u8; 4] =
            ficr.deviceaddr[0].read().deviceaddr().bits().to_le_bytes();
        let device_addr2: [u8; 4] =
            ficr.deviceaddr[1].read().deviceaddr().bits().to_le_bytes();

        let mut bytes = [0; 8];
        bytes[0..=3].copy_from_slice(&device_addr1[..4]);
        bytes[4..=5].copy_from_slice(&[0xFF, 0xFE]);
        bytes[6..=7].copy_from_slice(&device_addr2[..2]);
        Ieee802154Address(bytes)
    }
}
