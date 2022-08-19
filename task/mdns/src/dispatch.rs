use task_mdns_api::*;

use crate::server::idl::{MdnsOperation, InOrderMdnsImpl};
use idol_runtime::{Server, ServerOp};
use userlib::*;

pub fn dispatch<S: InOrderMdnsImpl>(buffer: &mut [u8], server: &mut S) {
    let mut server = (core::marker::PhantomData, server);
    // notification mask of aether is just 1
    // TODO maybe we should define that better somewhere?
    let rm = sys_recv_open(buffer, 1);

    // This is where we differ from the normal runtime,
    // we also want to return early if we get an incoming packet
    // and go to the main loop where we poll aether.
    if rm.sender == TaskId::KERNEL {
        return;
    }

    let op = match MdnsOperation::from_u32(rm.operation) {
        Some(op) => op,
        None => {
            sys_reply_fault(rm.sender, ReplyFaultReason::UndefinedOperation);
            return;
        }
    };

    if rm.message_len > buffer.len() {
        sys_reply_fault(rm.sender, ReplyFaultReason::BadMessageSize);
        return;
    }
    if rm.response_capacity < op.max_reply_size() {
        sys_reply_fault(rm.sender, ReplyFaultReason::ReplyBufferTooSmall);
        return;
    }

    let incoming = &buffer[..rm.message_len];

    if rm.lease_count != op.required_lease_count() {
        sys_reply_fault(rm.sender, ReplyFaultReason::BadLeases);
        return;
    }

    match server.handle(op, incoming, &rm) {
        Ok(()) => {
            // stub has taken care of it.
        }
        Err(idol_runtime::RequestError::Runtime(code)) => {
            // stub has used the convenience return for data-less errors,
            // we'll do the reply.
            sys_reply(rm.sender, code as u32, &[]);
        }
        Err(idol_runtime::RequestError::Fail(code)) => {
            if let Some(reason) = code.into_fault() {
                sys_reply_fault(rm.sender, reason);
            } else {
                // Cases like WentAway do not merit a reply.
            }
        }
    }
}
