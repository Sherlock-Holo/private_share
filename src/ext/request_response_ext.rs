use libp2p::request_response::{RequestId, RequestResponseEvent, RequestResponseMessage};

pub trait RequestResponseEventExt {
    fn request_id(&self) -> RequestId;
}

impl<Req, Resp, ChannelResp> RequestResponseEventExt
    for RequestResponseEvent<Req, Resp, ChannelResp>
{
    fn request_id(&self) -> RequestId {
        match self {
            RequestResponseEvent::Message { message, .. } => match message {
                RequestResponseMessage::Request { request_id, .. } => *request_id,
                RequestResponseMessage::Response { request_id, .. } => *request_id,
            },
            RequestResponseEvent::OutboundFailure { request_id, .. } => *request_id,
            RequestResponseEvent::InboundFailure { request_id, .. } => *request_id,
            RequestResponseEvent::ResponseSent { request_id, .. } => *request_id,
        }
    }
}
