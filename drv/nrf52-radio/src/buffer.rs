use nrf52840_pac as device;
use core::{
    cell::{UnsafeCell, Cell},
    sync::atomic::{AtomicUsize, AtomicBool, Ordering},
};
use userlib::sys_log;

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
         let resp = func(&mut buf[1..len-1]);
//         sys_log!("READ - {} | {:02X?}", len, &buf[1..len+1]);
         Some(resp)
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
        sys_log!("WRITE - {} | {:02X?}", len, &buf[1..len+1]);
        Some(resp)
    }
}
