// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use task_aether_api::*;
use userlib::*;

task_slot!(AETHER, aether);

#[export_name = "main"]
fn main() -> ! {
    let aether = AETHER.get_task_id();
    let aether = Aether::from(aether);

    const SOCKET: SocketName = SocketName::throughput;

    loop {
        // Tiiiiiny payload buffer
        let mut rx_data_buf = [0u8; 2048];
        match aether.recv_udp_packet(SOCKET, &mut rx_data_buf) {
            Ok(meta) => {
                // A packet! We want to turn it right around. Deserialize the
                // packet header; unwrap because we trust the server.
                UDP_ECHO_COUNT
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                // Now we know how many bytes to return.
                let tx_bytes = &rx_data_buf[..meta.payload_len as usize];

                loop {
                    match aether.send_udp_packet(SOCKET, meta, tx_bytes) {
                        Ok(()) => break,
                        Err(AetherError::QueueFull) => {
                            // Our outgoing queue is full; wait for space.
                            sys_recv_closed(&mut [], 1, TaskId::KERNEL)
                                .unwrap();
                        }
                        Err(AetherError::WrongOwner) => panic!(),
                        Err(_) => panic!(),
                    }
                }
            }
            Err(AetherError::QueueEmpty) => {
                // Our incoming queue is empty. Wait for more packets.
                sys_recv_closed(&mut [], 1, TaskId::KERNEL).unwrap();
            }
            Err(AetherError::WrongOwner) => panic!(),
            Err(_) => panic!(),
        }

        // Try again.
    }
}

static UDP_ECHO_COUNT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);