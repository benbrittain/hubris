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
use userlib::sys_log;

mod buffer;
mod phy;

use buffer::PacketBuffer;

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
    transmit_buffer: PacketBuffer,
    receive_buffer: PacketBuffer,
    mode: UnsafeCell<DriverState>,
}

impl Radio<'_> {
    pub fn new() -> Self {
        let radio = unsafe { &*device::RADIO::ptr() };

        Radio {
            radio,
            transmit_buffer: PacketBuffer::new(),
            receive_buffer: PacketBuffer::new(),
            mode: UnsafeCell::new(DriverState::Sleep),
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
    fn configure_packet_buffer(&self, buf: &PacketBuffer) {
        buf.set_as_buffer(&self);
    }

    /// IEEE 802.15.4 implements a listen-before-talk channel access method
    /// to avoid collisions when transmitting.
    fn configure_cca(&self) {
        // not sure if this is right, look more into this.
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

    /// IEEE 802.15.4 uses packets of max 127 bytes with a 32bit
    /// all-zero preamble and crc included as part of the frame.
    fn configure_packets(&self) {
        self.radio.mode.write(|w| w.mode().ieee802154_250kbit());

        self.radio.pcnf1.write(|w| unsafe { w.maxlen().bits(127) });
        self.radio.pcnf0.write(|w| unsafe {
            w.lflen().bits(8).plen()._32bit_zero().crcinc().set_bit()
        });
    }

    pub fn initialize(&self) {
        sys_log!("Initializing Radio...");
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
            .write(|w| unsafe { w.frequency().bits(15) });

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

        // TODO
        // enable fast ramp up
        self.radio.modecnf0.write(|w| w.ru().set_bit());

        // Disable MAC header matching
        // radio.mhrmatchconf.write(|w| unsafe { w.bits(0) });
        // radio.mhrmatchmas.write(|w| unsafe { w.bits(MHMU_MASK) });

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
        let idx = self.receive_buffer.completed.load(Ordering::Relaxed);
        idx >= 1
    }

    /// If we've gotten a packet, send it to smoltcp
    /// this does not read directly from the easyDMA buffer
    pub fn try_recv<R>(
        &self,
        read_buffer: impl FnOnce(&mut [u8]) -> R,
    ) -> Option<R> {
        sys_log!("Trying to receive...");
        self.receive_buffer.read(read_buffer)
    }

    pub fn can_send(&mut self) -> bool {
        // TODO check if buffer is full!!
        true
    }

    /// Tries to send a packet, if TX buffer space is available.
    pub fn try_send<R>(
        &self,
        len: usize,
        build_packet: impl FnOnce(&mut [u8]) -> R,
    ) -> Option<R> {
        sys_log!("Trying to send ...");
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
        sys_log!("starting transmit from state: {:?}", self.get_state());
        self.turn_off();
        self.set_mode(DriverState::CcaTx);

        self.configure_packet_buffer(&self.transmit_buffer);
        self.initialize();
    }

    /// Transition radio into receive state
    pub fn start_recv(&self) {
        sys_log!("Starting recieve... {:?}", self.get_state());
        self.radio.events_ready.reset();

        match self.get_driver_state() {
            // we always start recieving if starting from a sleep state
            DriverState::Sleep | DriverState::Rx => {
                self.set_mode(DriverState::Rx);
                self.configure_packet_buffer(&self.receive_buffer);
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
            // sys_log!("MODE - {:?} -> {:?}", old_mode, mode);
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
            sys_log!("IRQ - Wireless medium busy - do not send");
            self.radio.events_ccabusy.reset();
        }

        if self
            .radio
            .events_ccaidle
            .read()
            .events_ccaidle()
            .bit_is_set()
        {
            sys_log!("IRQ - Wireless medium in idle - clear to send");
            self.radio.events_ccaidle.reset();
            self.set_mode(DriverState::Tx);
            self.radio.tasks_txen.write(|w| w.tasks_txen().set_bit());
        }

        if self.radio.events_ready.read().events_ready().bit_is_set() {
            // this should always be triggered in conjunction with
            // a tx/rx event ready state
            sys_log!(
                "IRQ - RADIO has ramped up and is ready to be started {:?}",
                self.get_state()
            );
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
            sys_log!("IRQ - IEEE 802.15.4 length field received");
            self.radio.events_framestart.reset();
        }

        if self.radio.events_end.read().events_end().bit_is_set() {
            sys_log!("IRQ - Packet sent or received");
            self.radio.events_end.reset();

            match self.get_state() {
                RadioState::RxIdle => {
                    if self.radio.crcstatus.read().crcstatus().is_crcok() {
                        let buf: &[u8] =
                            unsafe { &*self.receive_buffer.data.get() };
                        sys_log!("CRC: OK!");

                        let mut buf =
                            unsafe { (*self.receive_buffer.data.get()) };
                        let len = buf[0] as usize;
                        if len > 0 {
                            self.receive_buffer
                                .completed
                                .fetch_add(1, Ordering::Relaxed);
                            // sys_log!("buf: {:02X?}", &buf[..len]);
                        }
                    } else {
                        sys_log!("CRC: BAD!");
                    }
                }
                RadioState::TxIdle => {
                    // transition back to Rx
                    self.set_mode(DriverState::Rx);
                }
                s => {
                    panic!("Don't know how to handle {:?} during event end", s)
                }
            }
            // TODO Don't do a full reset, transition modes here.
            self.turn_off();
            self.initialize();
        }

        self.enable_interrupts();
    }

    /// Generate a Extended Unique Identifier (RFC2373) from the FICR registers so the
    /// device can self-assign a unique 64-Bit IP Version 6 interface identifier (EUI-64).
    pub fn get_ieee_uei_64(&mut self) -> Ipv6Address {
        // TODO this isn't valid, I'm not an Org.
        pub const ORG_UNIQUE_IDENT: u32 = 0xb1eafb;
        let ficr = unsafe { &*device::FICR::ptr() };

        let device_addr1: [u8; 4] =
            ficr.deviceaddr[0].read().deviceaddr().bits().to_le_bytes();
        let device_addr2: [u8; 4] =
            ficr.deviceaddr[1].read().deviceaddr().bits().to_le_bytes();

        sys_log!(
            "MAC ADDR {}:{}:{}:{}:{}:{}",
            device_addr1[0],
            device_addr1[1],
            device_addr1[2],
            device_addr1[3],
            device_addr2[0],
            device_addr2[1]
        );
        let mut bytes = [0; 16];
        // Link-local address block.
        bytes[0..2].copy_from_slice(&[0xFE, 0x80]);
        // Bytes 2..8 are all zero.
        // Top three bytes of MAC address...
        bytes[8..11].copy_from_slice(&device_addr1[0..3]);

        // ...with administration scope bit flipped.
        bytes[8] ^= 0b0000_0010;

        // Inserted FF FE from EUI64 transform.
        bytes[11..13].copy_from_slice(&[0xFF, 0xFE]);

        // Bottom three bytes of MAC address.
        bytes[13] = device_addr1[3];
        bytes[14..16].copy_from_slice(&device_addr2[0..2]);

        Ipv6Address(bytes)
    }
}
