use crate::{RADIO_IRQ, TIMER_MASK};
use idol_runtime::{ClientError, Leased, NotificationHandler, RequestError};
use rand::Rng;
use smoltcp::iface::{Interface, SocketHandle, SocketSet};
use smoltcp::socket::dns::QueryHandle;
use smoltcp::socket::{dns, tcp, udp};
use smoltcp::wire::DnsQueryType;
use task_aether_api::{
    AetherError, Ieee802154Address, Ipv6Address, SocketName, TcpMetadata,
    UdpMetadata,
};

use userlib::*;

// TODO play around with this size
const TIMER_INTERVAL: u64 = 100;

/// Size of buffer that must be allocated to use `dispatch`.
pub const INCOMING_SIZE: usize = idl::INCOMING_SIZE;

#[derive(Clone, Copy)]
pub enum SocketHandleType {
    Udp(SocketHandle),
    Tcp(SocketHandle),
    Dns(SocketHandle),
}

pub struct AetherServer<'a> {
    socket_handles: [SocketHandleType; crate::generated::SOCKET_COUNT + 1],
    client_waiting_to_send: [bool; crate::generated::SOCKET_COUNT + 1],
    socket_set: SocketSet<'a>,
    iface: Interface<'a>,
    device: nrf52_radio::Radio<'a>,
    rng: drv_rng_api::Rng,
    dns_query: Option<QueryHandle>,
}

impl<'a> AetherServer<'a> {
    pub fn new(
        socket_handles: [SocketHandleType; crate::generated::SOCKET_COUNT + 1],
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
            dns_query: None,
            client_waiting_to_send: [false; crate::generated::SOCKET_COUNT + 1],
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

    /// Iterate over sockets, waking any that can do work.
    pub fn wake_sockets(&mut self) {
        // There's something to do! Iterate over sockets looking for work.
        // TODO making every packet O(n) in the number of sockets is super
        // lame; provide a Waker to fix this.
        for i in 0..crate::generated::SOCKET_COUNT {
            let want_to_send = self.client_waiting_to_send[i];
            let handle = self.socket_handles.get(i).cloned().unwrap();

            match handle {
                SocketHandleType::Udp(handle) => {
                    let socket = self.socket_set.get_mut::<udp::Socket>(handle);
                    if socket.can_recv() || (want_to_send && socket.can_send())
                    {
                        let (task_id, notification) =
                            crate::generated::SOCKET_OWNERS[i];
                        let task_id = sys_refresh_task_id(task_id);
                        sys_post(task_id, notification);
                    }
                }
                SocketHandleType::Tcp(handle) => {
                    let socket = self.socket_set.get_mut::<tcp::Socket>(handle);
                    if socket.can_recv() || (want_to_send && socket.can_send())
                    {
                        let (task_id, notification) =
                            crate::generated::SOCKET_OWNERS[i];
                        let task_id = sys_refresh_task_id(task_id);
                        sys_post(task_id, notification);
                    }
                    self.socket_set.get_mut::<tcp::Socket>(handle);
                }
                SocketHandleType::Dns(handle) => unreachable!(),
            };
        }
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
                e => Err(AetherError::Unknown.into()),
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
                self.socket_set
                    .get_mut::<tcp::Socket>(handle)
                    .connect(self.iface.context(), remote_ep, local_ep)
                    .map_err(|_| AetherError::TcpFailConnect)?;
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
        Ok(socket.may_send() && socket.may_recv())
    }

    fn send_tcp_data(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        payload: Leased<idol_runtime::R, [u8]>,
    ) -> Result<u32, RequestError<AetherError>> {
        let socket_index = socket as usize;
        if crate::generated::SOCKET_OWNERS[socket as usize].0.index()
            != msg.sender.index()
        {
            return Err(AetherError::WrongOwner.into());
        }

        let socket = self.get_tcp_socket_mut(socket as usize)?;

        match socket.send(|buf| {
            let len = if buf.len() < payload.len() {
                buf.len()
            } else {
                payload.len()
            };
            match payload.read_range(0..payload.len(), buf) {
                Ok(_) => (len, Ok(len as u32)),
                Err(e) => (0, Err(e)),
            }
        }) {
            Ok(Ok(len)) => {
                self.client_waiting_to_send[socket_index] = false;
                Ok(len)
            }
            //Err(smoltcp::Error::Exhausted) => {
            //    self.client_waiting_to_send[socket_index] = true;
            //    Err(AetherError::QueueFull.into())
            //}
            Ok(Err(_)) => Err(RequestError::Fail(ClientError::WentAway)),
            Err(_) => Err(AetherError::Unknown.into()),
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
        }
    }

    fn send_udp_packet(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: SocketName,
        metadata: UdpMetadata,
        payload: Leased<idol_runtime::R, [u8]>,
    ) -> Result<(), RequestError<AetherError>> {
        let socket_index = socket as usize;
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
                self.client_waiting_to_send[socket_index] = false;
                Ok(())
            }
            Err(udp::SendError::BufferFull) => {
                self.client_waiting_to_send[socket_index] = true;
                Err(AetherError::QueueFull.into())
            }
            e => panic!("couldn't send packet {:?}", e),
        }
    }

    fn get_addr(
        &mut self,
        _: &userlib::RecvMessage,
    ) -> Result<Ieee802154Address, idol_runtime::RequestError<AetherError>>
    {
        Ok(self.device.get_addr())
    }

    fn get_rssi(
        &mut self,
        _: &userlib::RecvMessage,
    ) -> Result<(), RequestError<AetherError>> {
        unimplemented!();
    }

    fn resolve_query(
        &mut self,
        _: &userlib::RecvMessage,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<AetherError>> {
        let dns_handle = match self.dns_query {
            Some(handle) => handle,
            None => return Err(AetherError::NoPendingDnsQuery.into()),
        };

        // We insert the DNS as the very last one
        let dns_idx = self.socket_handles.len() - 1;
        let handle = self
            .socket_handles
            .get(dns_idx)
            .cloned()
            .ok_or(RequestError::Fail(ClientError::BadMessageContents))?;

        if let SocketHandleType::Dns(handle) = handle {
            let dns = self.socket_set.get_mut::<dns::Socket>(handle);
            match dns.get_query_result(dns_handle) {
                Ok(names) => {
                    let name = names.get(0).unwrap();
                    match name {
                        smoltcp::wire::IpAddress::Ipv6(ipv6) => {
                            return Ok(Ipv6Address::from(*ipv6))
                        }
                    }
                }
                Err(dns::GetQueryResultError::Failed) => {
                    return Err(AetherError::DnsFailure.into())
                }
                Err(dns::GetQueryResultError::Pending) => {
                    return Err(AetherError::QueueEmpty.into())
                }
            }
        }
        panic!("Internal condiditon of DNS being last handle violated")
    }

    fn start_resolve_query(
        &mut self,
        _msg: &userlib::RecvMessage,
        url: idol_runtime::Leased<idol_runtime::R, [u8]>,
    ) -> Result<(), idol_runtime::RequestError<AetherError>> {
        let mut url_buf = [0; 256];
        url.read_range(0..url.len(), &mut url_buf[..url.len()])
            .unwrap();
        if self.dns_query.is_some() {
            return Err(AetherError::DnsQueryAlreadyInflight.into());
        }
        // We insert the DNS as the very last one
        let dns_idx = self.socket_handles.len() - 1;
        let handle = self
            .socket_handles
            .get(dns_idx)
            .cloned()
            .ok_or(RequestError::Fail(ClientError::BadMessageContents))?;

        if let SocketHandleType::Dns(handle) = handle {
            let dns = self.socket_set.get_mut::<dns::Socket>(handle);
            let handle = dns
                .start_query(
                    self.iface.context(),
                    core::str::from_utf8(&url_buf[..url.len()]).unwrap(),
                    DnsQueryType::Aaaa,
                )
                .map_err(|_| RequestError::went_away())?;
            self.dns_query = Some(handle);
            return Ok(());
            // and make this totally blocking.
        }
        panic!("Internal condiditon of DNS being last handle violated")
    }
}

impl NotificationHandler for AetherServer<'_> {
    fn current_notification_mask(&self) -> u32 {
        RADIO_IRQ | TIMER_MASK
    }

    fn handle_notification(&mut self, bits: u32) {
        // Interrupt dispatch.
        self.device.handle_interrupt();
        if bits & RADIO_IRQ != 0 {
            userlib::sys_irq_control(RADIO_IRQ, true);
        }
        if bits & TIMER_MASK != 0 {
            let deadline = sys_get_timer().now + TIMER_INTERVAL;
            sys_set_timer(Some(deadline), TIMER_MASK);
        }
    }
}

mod idl {
    use task_aether_api::*;
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
