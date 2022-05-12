use idol_runtime::{ClientError, Leased, NotificationHandler, RequestError};
use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::UdpSocket;

use task_aether_api::{AetherError, Ipv6Address, SocketName, UdpMetadata};
use userlib::*;

use crate::RADIO_IRQ;

/// Size of buffer that must be allocated to use `dispatch`.
pub const INCOMING_SIZE: usize = idl::INCOMING_SIZE;

pub struct AetherServer<'a> {
    socket_handles: [SocketHandle; crate::generated::SOCKET_COUNT],
    iface: Interface<'a, nrf52_radio::Radio<'a>>,
}

impl<'a> AetherServer<'a> {
    pub fn new(
        socket_handles: [SocketHandle; crate::generated::SOCKET_COUNT],
        iface: Interface<'a, nrf52_radio::Radio<'a>>,
    ) -> Self {
        Self {
            socket_handles,
            iface,
        }
    }

    /// Borrows a direct reference to the `smoltcp` `Interface` inside the
    /// server. This is exposed for use by the driver loop in main.
    pub fn interface_mut(
        &mut self,
    ) -> &mut Interface<'a, nrf52_radio::Radio<'a>> {
        &mut self.iface
    }
}

impl<'a> AetherServer<'a> {
    /// Gets the socket `index`. If `index` is out of range, returns
    /// `BadMessage`.
    ///
    /// All sockets are UDP.
    pub fn get_socket_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut UdpSocket<'a>, RequestError<AetherError>> {
        let handle = self
            .socket_handles
            .get(index)
            .cloned()
            .ok_or(RequestError::Fail(ClientError::BadMessageContents))?;
        Ok(self.iface.get_socket::<UdpSocket>(handle))
    }
}

impl idl::InOrderAetherImpl for AetherServer<'_> {
    fn recv_packet(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        payload: Leased<idol_runtime::W, [u8]>,
    ) -> Result<UdpMetadata, RequestError<AetherError>> {
        let socket_index = socket as usize;

        if crate::generated::SOCKET_OWNERS[socket_index].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_socket_mut(socket_index)?;
        match socket.recv() {
            Ok((body, endp)) => {
                if payload.len() < body.len() {
                    return Err(RequestError::Fail(ClientError::BadLease));
                }
                payload
                    .write_range(0..body.len(), body)
                    .map_err(|_| RequestError::went_away())?;

                Ok(UdpMetadata {
                    port: endp.port,
                    payload_len: body.len() as u32,
                    addr: endp.addr.try_into().map_err(|_| ()).unwrap(),
                })
            }
            Err(smoltcp::Error::Exhausted) => {
                Err(AetherError::QueueEmpty.into())
            }
            e => Err(AetherError::Unknown.into()),
        }
    }

    fn send_packet(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        metadata: UdpMetadata,
        payload: Leased<idol_runtime::R, [u8]>,
    ) -> Result<(), RequestError<AetherError>> {
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_socket_mut(socket as usize)?;

        match socket.send(payload.len(), metadata.into()) {
            Ok(buf) => {
                payload
                    .read_range(0..payload.len(), buf)
                    .map_err(|_| RequestError::went_away())?;
                Ok(())
            }
            Err(smoltcp::Error::Exhausted) => {
                Err(AetherError::NoTransmitSlot.into())
            }
            e => panic!("couldn't send packet {:?}", e),
        }
    }

    fn get_addr(
        &mut self,
        msg: &userlib::RecvMessage,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<AetherError>> {
        Ok(self.iface.device_mut().get_ieee_uei_64())
    }

    fn get_rssi(
        &mut self,
        msg: &userlib::RecvMessage,
    ) -> Result<(), RequestError<AetherError>> {
        unimplemented!();
    }
}

impl NotificationHandler for AetherServer<'_> {
    fn current_notification_mask(&self) -> u32 {
        RADIO_IRQ
    }

    fn handle_notification(&mut self, bits: u32) {
        // Interrupt dispatch.
        if bits & RADIO_IRQ != 0 {
            self.iface.device_mut().handle_interrupt();
            userlib::sys_irq_control(RADIO_IRQ, true);
        }
    }
}
mod idl {
    use task_aether_api::{AetherError, Ipv6Address, SocketName, UdpMetadata};
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
