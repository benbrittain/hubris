use idol_runtime::{ClientError, Leased, NotificationHandler, RequestError};
use smoltcp::iface::{Interface, SocketHandle, SocketSet};
use smoltcp::socket::{tcp, udp};
use crate::RADIO_IRQ;
use rand::Rng;
use task_aether_api::{
    AetherError, Ieee802154Address, Ipv6Address, SocketName, TcpMetadata,
    UdpMetadata,
};

use userlib::*;

/// Size of buffer that must be allocated to use `dispatch`.
pub const INCOMING_SIZE: usize = idl::INCOMING_SIZE;

#[derive(Clone, Copy)]
pub enum SocketHandleType {
    Udp(SocketHandle),
    Tcp(SocketHandle),
}

pub struct AetherServer<'a> {
    socket_handles: [SocketHandleType; crate::generated::SOCKET_COUNT],
    socket_set: SocketSet<'a>,
    iface: Interface<'a>,
    device: nrf52_radio::Radio<'a>,
    rng: drv_rng_api::Rng,
}

impl<'a> AetherServer<'a> {
    pub fn new(
        socket_handles: [SocketHandleType; crate::generated::SOCKET_COUNT],
        socket_set: SocketSet<'a>,
        iface: Interface<'a>,
        device: nrf52_radio::Radio<'a>,
        rng: drv_rng_api::Rng,
    ) -> Self {
        Self {
            socket_handles,
            socket_set,
            iface,
            device,
            rng,
        }
    }

    /// Poll the `smoltcp` `Interface`
    pub fn poll(
        &mut self,
        time: smoltcp::time::Instant,
    ) -> Result<bool, smoltcp::Error> {
        self.iface
            .poll(time, &mut self.device, &mut self.socket_set)
    }

    /// Gets the udp socket `index`. If `index` is out of range, returns
    /// `BadMessage`. If the socket is not udp, error
    pub fn get_udp_socket_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut udp::Socket<'a>, RequestError<AetherError>> {
        let handle = self
            .socket_handles
            .get(index)
            .cloned()
            .ok_or(RequestError::Fail(ClientError::BadMessageContents))?;
        match handle {
            SocketHandleType::Udp(handle) => {
                Ok(self.socket_set.get_mut::<udp::Socket>(handle))
            }
            _ => Err(AetherError::WrongSocketType.into()),
        }
    }
    /// Gets the udp socket `index`. If `index` is out of range, returns
    /// `BadMessage`. If the socket is not udp, error
    pub fn get_tcp_socket_mut(
        &mut self,
        index: usize,
    ) -> Result<&mut tcp::Socket<'a>, RequestError<AetherError>> {
        let handle = self
            .socket_handles
            .get(index)
            .cloned()
            .ok_or(RequestError::Fail(ClientError::BadMessageContents))?;
        match handle {
            SocketHandleType::Tcp(handle) => {
                Ok(self.socket_set.get_mut::<tcp::Socket>(handle))
            }
            _ => Err(AetherError::WrongSocketType.into()),
        }
    }
}

impl idl::InOrderAetherImpl for AetherServer<'_> {
    fn recv_tcp_data(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        payload: Leased<idol_runtime::W, [u8]>,
    ) -> Result<u32, RequestError<AetherError>> {
        let socket_index = socket as usize;

        if crate::generated::SOCKET_OWNERS[socket_index].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }
        let socket = self.get_tcp_socket_mut(socket_index)?;
        if socket.may_recv() {
            match socket.recv(|data| (data.len(), data)) {
                Ok(data) => {
                    if data.len() == 0 {
                        Err(AetherError::QueueEmpty.into())
                    } else {
                        if payload.len() < data.len() {
                            return Err(RequestError::Fail(
                                ClientError::BadLease,
                            ));
                        }
                        payload
                            .write_range(0..data.len(), data)
                            .map_err(|_| RequestError::went_away())?;
                        Ok(data.len() as u32)
                    }
                }
                e => {
                    sys_log!("got an unknown error: {:?}", e);
                    Err(AetherError::Unknown.into())
                }
            }
        } else if socket.may_send() {
            Err(AetherError::RemoteTcpClose.into())
        } else {
            Err(AetherError::QueueEmpty.into())
        }
    }

    fn tcp_listen(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        port: u16,
    ) -> Result<(), RequestError<AetherError>> {
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_tcp_socket_mut(socket as usize)?;
        // TODO consider using close and a seperate abort function?
        socket.listen(port).map_err(|_| AetherError::Unknown.into())
    }

    fn tcp_connect(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        metadata: TcpMetadata,
    ) -> Result<(), RequestError<AetherError>> {
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        // NOTE
        // This doesn't use get_tcp_socket_mut because of double borrow problem
        // This can probably be refactored to be cleaner
        let remote_ep =
            smoltcp::wire::IpEndpoint::from((metadata.addr, metadata.port));
        let local_ep: u16 = self.rng.gen_range(1024..65535);
        let handle = self
            .socket_handles
            .get(socket as usize)
            .cloned()
            .ok_or(RequestError::Fail(ClientError::BadMessageContents))?;
        match handle {
            SocketHandleType::Tcp(handle) => {
                self.socket_set.get_mut::<tcp::Socket>(handle).connect(
                    self.iface.context(),
                    remote_ep,
                    local_ep,
                );
                Ok(())
            }
            _ => Err(AetherError::WrongSocketType.into()),
        }
    }

    fn close_tcp(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
    ) -> Result<(), idol_runtime::RequestError<AetherError>> {
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_tcp_socket_mut(socket as usize)?;
        // TODO consider using close and a seperate abort function?
        socket.abort();
        Ok(())
    }

    fn is_tcp_active(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
    ) -> Result<bool, idol_runtime::RequestError<AetherError>> {
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_tcp_socket_mut(socket as usize)?;
        Ok(socket.may_send())
    }

    fn send_tcp_data(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        payload: Leased<idol_runtime::R, [u8]>,
    ) -> Result<u32, RequestError<AetherError>> {
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_tcp_socket_mut(socket as usize)?;

        match socket.send(|buf| {
            if buf.len() < payload.len() {
                panic!("buffer stuff to do ben!");
            }
            payload.read_range(0..payload.len(), buf).unwrap();
            // TODO there needs to be a way of handling if this write fails

            (payload.len(), payload.len() as u32)
        }) {
            Ok(len) => Ok(len),
            e => {
                sys_log!("couldn't send packet {:?}", e);
                panic!("couldn't send packet {:?}", e);
            },
        }
    }

    fn recv_udp_packet(
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

        let socket = self.get_udp_socket_mut(socket_index)?;
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
                    addr: endp.addr.try_into().unwrap(),
                })
            }
            Err(udp::RecvError::Exhausted) => {
                Err(AetherError::QueueEmpty.into())
            }
            e => Err(AetherError::Unknown.into()),
        }
    }

    fn send_udp_packet(
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

        let socket = self.get_udp_socket_mut(socket as usize)?;

        match socket.send(payload.len(), metadata.into()) {
            Ok(buf) => {
                payload
                    .read_range(0..payload.len(), buf)
                    .map_err(|_| RequestError::went_away())?;
                Ok(())
            }
            Err(udp::SendError::BufferFull) => {
                Err(AetherError::NoTransmitSlot.into())
            }
            e => panic!("couldn't send packet {:?}", e),
        }
    }

    fn get_addr(
        &mut self,
        msg: &userlib::RecvMessage,
    ) -> Result<Ieee802154Address, idol_runtime::RequestError<AetherError>>
    {
        Ok(self.device.get_addr())
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
            self.device.handle_interrupt();
            userlib::sys_irq_control(RADIO_IRQ, true);
        }
    }
}
mod idl {
    use task_aether_api::{
        AetherError, Ieee802154Address, SocketName, TcpMetadata, UdpMetadata,
    };
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
