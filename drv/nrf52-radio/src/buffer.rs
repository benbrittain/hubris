use core::{
    cell::{Cell, UnsafeCell},
    sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering},
};
use nrf52840_pac as device;
use userlib::sys_log;


/// The maximum size of a 802.15.4 packet payload.
const MAX_PACKET_SIZE: usize = 256;
/// The length of the phy length field in bytes. beautiful.
const PHY_LEN_LEN: usize = 1;
/// The length of the crc field in bytes
const CRC_LEN: usize = 1;

const BUF_SIZE: usize = MAX_PACKET_SIZE * 3;

pub struct RecvPacketBuffer {
    /// a Ring Buffer.
    pub data: UnsafeCell<[u8; BUF_SIZE]>,
    /// Offset of the packet being *read into* smoltcp.
    pub read_packet: Cell<isize>,
    /// Offset of the packet being *written into* by the EasyDMA engine.
    pub next_packet: Cell<isize>,
}

impl RecvPacketBuffer {
    pub const fn new() -> Self {
        Self {
            data: UnsafeCell::new([0; BUF_SIZE]),
            read_packet: Cell::new(0),
            next_packet: Cell::new(0),
        }
    }

    // TODO we need to internally check this is never called on a memory region the EasyDMA
    // engine is touching OR mark this as unsafe
    pub fn read<R>(&self, func: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        let read_packet = self.read_packet.get() as usize;
        // DMA engine stores a length at the first write.
        let len = unsafe { (*self.data.get())[read_packet] as usize };
        assert!(len != 0);
        sys_log!("\nlen: {}", len);
        assert!(len < MAX_PACKET_SIZE);

        // // Construct a slice of the Mac Data Protocol Unit
        let mpdu_slice = unsafe {
            &mut (*self.data.get())
                [read_packet + PHY_LEN_LEN..read_packet + len - CRC_LEN]
        };

        let frame  = smoltcp::wire::ieee802154::Frame::new_checked(&mpdu_slice);
        sys_log!("{}", frame.unwrap());
        // sys_log!("READ - {} | {:02X?}", len, &mut mpdu_slice[..]);
        let resp = func(mpdu_slice);

        self.move_read_packet(len as isize);

        // TODO REMOVE THIS

        //mpdu_slice.fill(0xAA);
        //sys_log!("RESET MPDU");
        //let mpdu_slice = unsafe {
        //    &mut (*self.data.get())
        //        [read_packet + PHY_LEN_LEN..read_packet + len - CRC_LEN]
        //};
        //sys_log!("READ - {} | {:02X?}", len, &mut mpdu_slice[..]);

        Some(resp)
    }

    /// Takes in the value of the PHR_LEN field updating where
    /// the buffer will write to next.
    fn move_next_packet(&self, phy_payload_len: isize) {
        let buf_len: isize = unsafe { (*self.data.get()).len() as isize };

        let next_packet: isize = self.next_packet.get();

        let next_offset = next_packet + phy_payload_len + 1;
        self.next_packet.set(if next_offset + MAX_PACKET_SIZE as isize >= buf_len {
            // we need to overflow around since it's possible a packet could be written
            // which wouldn't fit.
            0
        } else {
            next_offset
        });

        // sys_log!("NEW PACKET OFFSET: {}", self.next_packet.get());
    }

    /// Takes in the value of the PHR_LEN field updating where
    /// the buffer will read from next.
    fn move_read_packet(&self, phy_payload_len: isize) {
        let buf_len: isize = unsafe { (*self.data.get()).len() as isize };

        let read_packet: isize = self.read_packet.get();

        let next_offset = read_packet + phy_payload_len + 1;
        self.read_packet.set(if next_offset + MAX_PACKET_SIZE  as isize >= buf_len {
            0
        } else {
            next_offset
        });
    }

    pub fn got_packet(&self) {
        let mut read_packet = self.read_packet.get() as usize;

        // DMA engine stores a length at the first write.
        let len = unsafe { (*self.data.get())[read_packet as usize] as isize };

        // sys_log!("GOT PACKET - {} | {:02X?}", len, &view[..50]);

        if len >= 3 {
            let mpdu_slice = unsafe {
                &mut (*self.data.get())
                    [read_packet + PHY_LEN_LEN..read_packet + len as usize - CRC_LEN]
            };
            if let Ok(frame) = smoltcp::wire::ieee802154::Frame::new_checked(&mpdu_slice) {
                sys_log!("{} | {}", len, frame);
                self.move_next_packet(len);
            }
        }
    }

    pub fn has_packets(&self) -> bool {
        let read_packet: isize = self.read_packet.get();
        let next_packet: isize = self.next_packet.get();

        // sys_log!("bo: {} np: {}", read_packet, next_packet);
        // Technically we could have packets if we looped all the way around
        // to the exact some spot, but that's fairly unlikely and the buffer
        // is dropping things at that points, so like, best effort.
        next_packet != read_packet
    }

    pub fn set_as_buffer(&self, radio: &crate::Radio) {
        let read_packet: usize = self.read_packet.get() as usize;

        // we're gonna do something that looks a lot like aliasing here
        // it is. I'm so sorry, but that's why we put it behind an UnsafeCell.
        //
        // the EasyDMA engine wants pointers into the buffer.
        let buffer_ptr = unsafe {
            (&mut (*self.data.get())
                [read_packet..read_packet + 1])
                .as_mut_ptr()
        };
        //sys_log!("set up a new buffer: 0x{:p}", buffer_ptr);
        unsafe {
            radio
                .radio
                .packetptr
                .write(|w| w.packetptr().bits(buffer_ptr as u32));
        }
    }

    //pub fn write<R>(
    //    &self,
    //    func: impl FnOnce(&mut [u8]) -> R,
    //    len: usize,
    //) -> Option<R> {
    //    let mut buf = unsafe { &mut *self.data.get() };
    //    let resp = func(&mut buf[1..len + 1]);
    //    // set the phdr
    //    buf[0] = len as u8 + 2;
    //    sys_log!("WRITE - {} | {:02X?}", len, &buf[1..len + 1]);
    //    Some(resp)
    //}
}

pub struct PacketBuffer {
    pub data: UnsafeCell<[u8; 128]>,
    pub completed: AtomicUsize,
}

impl PacketBuffer {
    pub const fn new() -> Self {
        Self {
            data: UnsafeCell::new([0; 128]),
            completed: AtomicUsize::new(0),
        }
    }

    pub fn read<R>(&self, func: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        let mut buf = unsafe { (*self.data.get()) };
        let idx = self.completed.fetch_sub(1, Ordering::Relaxed);
        if idx > 1 {
            panic!("{} packets dropped!", idx - 1);
        }
        let len = buf[0] as usize;
        let resp = func(&mut buf[1..len - 1]);
        //         sys_log!("READ - {} | {:02X?}", len, &buf[1..len+1]);
        Some(resp)
    }

    pub fn set_as_buffer(&self, radio: &crate::Radio) {
        let buffer_ptr = self.data.get() as *mut _ as u32;
        // TODO consider doing some verification here
        // since unlike many pac unsafe usages, this is actually
        // very unsafe.
        unsafe {
            radio
                .radio
                .packetptr
                .write(|w| w.packetptr().bits(buffer_ptr));
        }
    }

    pub fn write<R>(
        &self,
        func: impl FnOnce(&mut [u8]) -> R,
        len: usize,
    ) -> Option<R> {
        let mut buf = unsafe { &mut *self.data.get() };
        let resp = func(&mut buf[1..len + 1]);
        // set the phdr
        buf[0] = len as u8 + 2;
        // sys_log!("WRITE - {} | {:02X?}", len, &buf[1..len + 1]);
        Some(resp)
    }
}
