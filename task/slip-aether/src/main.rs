// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A SLIP Interface to the Aether

#![no_std]
#![no_main]

use drv_nrf52_uart_api::Uart;
use serial_line_ip::Decoder;
use smoltcp::wire::Ipv6Repr;
use task_aether_api::*;
use userlib::*;

task_slot!(UART, uart);
task_slot!(AETHER, aether);

const SOCKET: SocketName = SocketName::slip;

#[export_name = "main"]
fn main() -> ! {
    let uart = Uart::from(UART.get_task_id());
    let aether = Aether::from(AETHER.get_task_id());

    sys_log!("SLIP into the Aether");

    let mut output = [0; 2048];
    let mut buffer = [0; 2048];
    loop {
        let amount_read = uart.read(0, &mut buffer).unwrap();
        if amount_read > 0 {
            sys_log!("=========");
//            sys_log!("in: {:x?}", &buffer[..amount_read]);
        }
        let mut slip = Decoder::new();
        if let Ok((bytes_processed, output_slice, is_end_of_packet)) =
            slip.decode(&buffer[..amount_read], &mut output)
        {
            if is_end_of_packet {
                match smoltcp::wire::Ipv6Packet::new_checked(output_slice) {
                    Ok(packet) => {
                        let repr = Ipv6Repr::parse(&packet).unwrap();
                        let mut dest_addr = repr.dst_addr;

                        let udp_packet = smoltcp::wire::UdpPacket::new_checked(
                            packet.payload(),
                        )
                        .unwrap();

                        let tx_bytes = udp_packet.payload();

                        sys_log!("swaping ip addr");
                        sys_log!("original dst: {:x?}", dest_addr.0);
                        dest_addr.0[0] = 0xfe;
                        dest_addr.0[1] = 0x80;
                        dest_addr.0[2] = 0x00;
                        dest_addr.0[3] = 0x00;
                        sys_log!("new dst: {:x?}", dest_addr.0);

                        let meta = UdpMetadata {
                            addr: Ipv6Address(dest_addr.0),
                            port: udp_packet.dst_port(),
                            payload_len: udp_packet.payload().len() as u32,
                        };
                        sys_log!("sending: {:?}", meta);

                        aether.send_packet(SOCKET, meta, &tx_bytes);
                        hl::sleep_for(100);
                        let mut rx_data_buf = [0u8; 64];
                        sys_log!("REC: {:?}", aether.recv_packet(SOCKET, &mut rx_data_buf));
                    }
                    Err(e) => sys_log!("err: {:?}", e),
                }
            }
        }
        hl::sleep_for(1000);
    }

}
