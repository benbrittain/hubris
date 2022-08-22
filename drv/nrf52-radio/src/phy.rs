use crate::Radio;
use smoltcp::{
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    time::Instant,
    Result,
};

pub struct RadioRxToken<'a>(&'a Radio<'a>);
pub struct RadioTxToken<'a>(&'a Radio<'a>);

impl<'a> Device<'a> for Radio<'_> {
    type RxToken = RadioRxToken<'a>;
    type TxToken = RadioTxToken<'a>;

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        // if the buffers aren't full, we attempt to receive packets
        if self.can_recv() && self.can_send() {
            return Some((RadioRxToken(self), RadioTxToken(self)));
        }
        None
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        if self.can_send() {
            return Some(RadioTxToken(self));
        }
        None
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 123;
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ieee802154;
        caps
    }
}

impl<'a> RxToken for RadioRxToken<'a> {
    fn consume<R, F>(self, _timestamp: Instant, f: F) -> Result<R>
    where
        F: FnOnce(&mut [u8]) -> Result<R>,
    {
        self.0.try_recv(f).expect("failed to recive packet")
    }
}

impl<'a> TxToken for RadioTxToken<'a> {
    fn consume<R, F>(self, _timestamp: Instant, len: usize, f: F) -> Result<R>
    where
        F: FnOnce(&mut [u8]) -> Result<R>,
    {
        self.0.try_send(len, f).expect("failed to transmit packet")
    }
}
