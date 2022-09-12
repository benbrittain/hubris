// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use minimq::{
    embedded_nal::{nb::Error as MqError, IpAddr, SocketAddr, TcpClientStack},
    embedded_time::fraction::Fraction,
};
use task_aether_api::*;
use userlib::*;

pub struct NetworkLayer {
    pub aether: Aether,
    pub socket: SocketName,
}

fn socket_addr_to_metadata(
    addr: SocketAddr,
) -> Result<TcpMetadata, AetherError> {
    if let SocketAddr::V6(addrv6) = addr {
        Ok(TcpMetadata {
            addr: Ipv6Address(addrv6.ip().octets()),
            port: addrv6.port(),
        })
    } else {
        Err(AetherError::NotIpv6)
    }
}

impl TcpClientStack for NetworkLayer {
    type TcpSocket = SocketName;
    type Error = AetherError;

    fn socket(
        &mut self,
    ) -> Result<
        <Self as TcpClientStack>::TcpSocket,
        <Self as TcpClientStack>::Error,
    > {
        Ok(self.socket)
    }

    fn connect(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        addr: SocketAddr,
    ) -> Result<(), MqError<<Self as TcpClientStack>::Error>> {
        self.aether
            .tcp_connect(self.socket, socket_addr_to_metadata(addr)?)
            .map_err(|e| MqError::Other(e))
    }

    fn is_connected(
        &mut self,
        socket: &<Self as TcpClientStack>::TcpSocket,
    ) -> Result<bool, <Self as TcpClientStack>::Error> {
        userlib::hl::sleep_for(200);
        self.aether.is_tcp_active(*socket)
    }

    fn send(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        bytes: &[u8],
    ) -> Result<usize, MqError<<Self as TcpClientStack>::Error>> {
        match self.aether.send_tcp_data(self.socket, bytes) {
            Ok(len) => {
                return Ok(len as usize);
            }
            Err(AetherError::QueueFull) => {
                // Our incoming queue is empty. Wait for more packets.
                return Err(MqError::WouldBlock);
            }
            Err(e) => {
                return Err(MqError::Other(e));
            }
        }
    }

    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        bytes: &mut [u8],
    ) -> Result<usize, MqError<<Self as TcpClientStack>::Error>> {
        // block this task until we get back the tcp data
        match self.aether.recv_tcp_data(self.socket, bytes) {
            Ok(len) => {
                return Ok(len as usize);
            }
            Err(AetherError::QueueEmpty) => {
                // Our incoming queue is empty. Wait for more packets.
                return Err(MqError::WouldBlock);
            }
            Err(e) => {
                return Err(MqError::Other(e));
            }
        }
    }

    fn close(
        &mut self,
        socket: <Self as TcpClientStack>::TcpSocket,
    ) -> Result<(), <Self as TcpClientStack>::Error> {
        self.aether.close_tcp(self.socket)
    }
}
