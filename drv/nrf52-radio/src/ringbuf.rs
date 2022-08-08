use core::cell::{Cell, UnsafeCell};
use core::iter::FromIterator;
use core::mem;
use core::mem::MaybeUninit;
use core::ops::{Index, IndexMut};

/// The maximum size of a 802.15.4 packet payload.
const PACKET_SIZE: usize = 255;

/// The length of the phy length field in bytes. beautiful.
const PHY_LEN_LEN: usize = 1;

/// The length of the crc field in bytes
const CRC_LEN: usize = 1;

type PacketBuf = [u8; PACKET_SIZE];

#[inline]
const fn mask(cap: usize, index: usize) -> usize {
    index & (cap - 1)
}

#[derive(Debug)]
pub struct RingBufferRx<const CAP: usize> {
    buf: UnsafeCell<[MaybeUninit<PacketBuf>; CAP]>,
    read_idx: Cell<usize>,
    write_idx: Cell<usize>,
}

impl<const CAP: usize> RingBufferRx<CAP> {
    pub fn new() -> Self {
        assert_ne!(CAP, 0, "Capacity must be greater than 0");
        assert!(CAP.is_power_of_two(), "Capacity must be a power of two");

        // TODO remove array_init
        let arr = array_init::array_init(|_| MaybeUninit::uninit());

        Self {
            buf: UnsafeCell::new(arr),
            write_idx: Cell::new(0),
            read_idx: Cell::new(0),
        }
    }

    /// Provide a buffer to the radio EasyDMA engine.
    pub fn set_as_buffer(&self, radio: &crate::Radio) {
        let index = mask(CAP, self.write_idx.get());
        let buffer_ptr =
            unsafe { (&mut (*self.buf.get())[index]).as_mut_ptr() };
        unsafe {
            radio
                .radio
                .packetptr
                .write(|w| w.packetptr().bits(buffer_ptr as u32));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn len(&self) -> usize {
        self.write_idx.get() - self.read_idx.get()
    }

    /// Move the write pointer IFF we got a packet
    pub fn got_packet(&self) {
        self.write_idx.set(self.write_idx.get() + 1);
    }

    pub fn read<R>(&self, func: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        if self.is_empty() {
            None
        } else {
            let index = mask(CAP, self.read_idx.get());

            // TODO safety description
            let mut slice =
                unsafe { (&mut (*self.buf.get())[index]).assume_init() };
            let packet_len = slice[0] as usize;

            assert!(packet_len != 0);
            assert!(packet_len < PACKET_SIZE);

            let mpdu_slice = &mut slice[PHY_LEN_LEN..packet_len - CRC_LEN];
            let frame =
                smoltcp::wire::ieee802154::Frame::new_checked(&mpdu_slice);
            // userlib::sys_log!("Read: {:?}", frame);
            self.read_idx.set(self.read_idx.get() + 1);

            Some(func(mpdu_slice))
        }
    }
}

#[derive(Debug)]
#[repr(C, align(32))]
pub struct RingBufferTx<const CAP: usize> {
    buf: UnsafeCell<[PacketBuf; CAP]>,
    read_idx: Cell<usize>,
    write_idx: Cell<usize>,
}

impl<const CAP: usize> RingBufferTx<CAP> {
    pub fn new() -> Self {
        assert_ne!(CAP, 0, "Capacity must be greater than 0");
        assert!(CAP.is_power_of_two(), "Capacity must be a power of two");
        let arr = [[0; 255]; CAP];

        Self {
            buf: UnsafeCell::new(arr),
            write_idx: Cell::new(0),
            read_idx: Cell::new(0),
        }
    }

    /// Provide a buffer to the radio EasyDMA engine.
    pub fn set_as_buffer(&self, radio: &crate::Radio) {
        let index = mask(CAP, self.read_idx.get());
        let buffer =
            unsafe { (&mut (*self.buf.get())[index]) };
        let len = buffer[0];
        cortex_m::asm::dsb();
        cortex_m::asm::dmb();
        cortex_m::asm::isb();
        assert!(len <= 127);
        let buffer_ptr =
            unsafe { (&mut (*self.buf.get())[index]).as_mut_ptr() };
        unsafe {
            radio
                .radio
                .packetptr
                .write(|w| w.packetptr().bits(buffer_ptr as u32));
        }
        cortex_m::asm::dsb();
    }

    pub fn is_full(&self) -> bool {
//        userlib::sys_log!("is full len {}", self.len());
        self.len() == CAP
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn len(&self) -> usize {
        self.write_idx.get() - self.read_idx.get()
    }

    /// Move the write pointer IFF we got a packet
    pub fn sent_packet(&self) {
//        userlib::sys_log!("~~~~~~~~~~~~~~ SENT PACKET {} ~~~~~~~~~~~~~~", self.read_idx.get());
        self.read_idx.set(self.read_idx.get() + 1);
    }

    pub fn write<R>(
        &self,
        func: impl FnOnce(&mut [u8]) -> R,
        len: usize,
    ) -> Option<R> {
        let index = mask(CAP, self.write_idx.get());
        let mut packet_buf =
            unsafe { (&mut (*self.buf.get())[index]) };
        cortex_m::asm::dsb();
        cortex_m::asm::dmb();
        cortex_m::asm::isb();

        let resp = func(&mut packet_buf[1..len + 1]);
        // set the phdr
        packet_buf[0] = len as u8 + 2;
        cortex_m::asm::dsb();
        cortex_m::asm::dmb();
        cortex_m::asm::isb();
        self.write_idx.set(self.write_idx.get() + 1);
        Some(resp)
    }
}
