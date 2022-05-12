use idol_runtime::{ClientError, Leased, NotificationHandler, RequestError};
use smoltcp::iface::{Interface, SocketHandle};
use smoltcp::socket::UdpSocket;

use task_aether_api::{AetherError, UdpMetadata, Ipv6Address};
use userlib::*;

use crate::RADIO_IRQ;

/// Size of buffer that must be allocated to use `dispatch`.
pub const INCOMING_SIZE: usize = idl::INCOMING_SIZE;

pub struct AetherServer<'a> {
    socket_handles: [SocketHandle; crate::SOCKET_COUNT],
    iface: Interface<'a, nrf52_radio::Radio<'a>>,
}

impl<'a> AetherServer<'a> {
    pub fn new(
        socket_handles: [SocketHandle; crate::SOCKET_COUNT],
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
        payload: Leased<idol_runtime::W, [u8]>,
    ) -> Result<UdpMetadata, RequestError<AetherError>> {
        // TODO this has to change when we support multiple clients!!
        let socket_index = 0;

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
            e => panic!("recv_packet error = {:?}", e),
        }
    }

    fn send_packet(
        &mut self,
        msg: &userlib::RecvMessage,
        metadata: UdpMetadata,
        payload: Leased<idol_runtime::R, [u8]>,
    ) -> Result<(), RequestError<AetherError>> {
        let socket_index = 0;
        let socket = self.get_socket_mut(socket_index)?;
        userlib::sys_log!("{:?}", socket.endpoint());

        match socket.send(payload.len(), metadata.into()) {
            Ok(buf) => {
                payload
                    .read_range(0..payload.len(), buf)
                    .map_err(|_| RequestError::went_away())?;
                Ok(())
            }
            e => panic!("couldn't send packet {:?}", e),
        }
    }

    fn get_addr(
        &mut self,
        msg: &userlib::RecvMessage,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<AetherError>>{
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
    use task_aether_api::{AetherError, UdpMetadata, Ipv6Address};
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
