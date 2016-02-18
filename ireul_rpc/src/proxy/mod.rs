extern crate time;

use std::sync::mpsc::{
    self,
    SyncSender,
    TrySendError,
    RecvError
};

pub mod track;

pub use self::track::{
    EnqueueTrackRequest,
    EnqueueTrackResult,
    EnqueueTrackError,
    FastForward,
    FastForwardRequest,
    FastForwardResult,
    FastForwardError,
    ReplaceFallbackRequest,
    ReplaceFallbackResult,
    ReplaceFallbackError,
};

// pub const SIZE_LIMIT: bincode::SizeLimit = bincode::SizeLimit::Bounded(20 * 1 << 20);

pub const OP_ENQUEUE_TRACK: u32 = 0x1000;
pub const OP_FAST_FORWARD: u32 = 0x1001;
pub const OP_QUEUE_STATUS: u32 = 0x1002;
pub const OP_REPLACE_FALLBACK: u32 = 0x1003;

pub enum RequestType {
    EnqueueTrack,
    FastForward,
    QueueStatus,
    ReplaceFallback,
}

impl RequestType {
    pub fn from_op_code(op_code: u32) -> Result<RequestType, ()> {
        match op_code {
            OP_ENQUEUE_TRACK => Ok(RequestType::EnqueueTrack),
            OP_FAST_FORWARD => Ok(RequestType::FastForward),
            OP_QUEUE_STATUS => Ok(RequestType::QueueStatus),
            OP_REPLACE_FALLBACK => Ok(RequestType::ReplaceFallback),
            _ => Err(())
        }
    }

    pub fn to_op_code(&self) -> u32 {
        match *self {
            RequestType::EnqueueTrack => OP_ENQUEUE_TRACK,
            RequestType::FastForward => OP_FAST_FORWARD,
            RequestType::QueueStatus => OP_QUEUE_STATUS,
            RequestType::ReplaceFallback => OP_REPLACE_FALLBACK,
        }
    }
}


pub struct RequestWrapper {
    // the Vec<u8>s are a bincode serialized representation
    pub response_queue: mpsc::SyncSender<Vec<u8>>,
    pub req_type: RequestType,
    pub req_buf: Vec<u8>,
}

pub type BinderResult<T, E> = Result<T, BinderError<E>>;

// wire-safe error wrapper. converted to ProxyError afterwards.

pub enum BinderError<T> {
    CallError(T),
    StubImplementation,
    RemoteSerdeError,
}

impl<T> From<ProxyError<T>> for BinderError<T> {
    fn from(e: ProxyError<T>) -> Self {
        match e {
            ProxyError::CallError(val) => BinderError::CallError(val),
            ProxyError::StubImplementation => BinderError::StubImplementation,
            ProxyError::RemoteSerdeError => BinderError::RemoteSerdeError,
            ProxyError::RpcError(_) => BinderError::RemoteSerdeError,
        }
    }
}

pub type ProxyResult<T, E> = Result<T, ProxyError<E>>;

pub enum ProxyError<T> {
    CallError(T),
    StubImplementation,
    RemoteSerdeError,
    RpcError(RpcError),
}

impl<T> From<BinderError<T>> for ProxyError<T> {
    fn from(e: BinderError<T>) -> Self {
        match e {
            BinderError::CallError(val) => ProxyError::CallError(val),
            BinderError::StubImplementation => ProxyError::StubImplementation,
            BinderError::RemoteSerdeError => ProxyError::RemoteSerdeError,
        }
    }
}

impl<T> From<RpcError> for ProxyError<T> {
    fn from(e: RpcError) -> Self {
        ProxyError::RpcError(e)
    }
}

pub enum RpcError {
    SendError(TrySendError<()>),
    RecvError(RecvError),
    SerializeError,
    DeserializeError,
}

impl<T> From<TrySendError<T>> for RpcError {
    fn from(e: TrySendError<T>) -> Self {
        match e {
            TrySendError::Disconnected(_) => {
                RpcError::SendError(TrySendError::Disconnected(()))
            },
            TrySendError::Full(_) => {
                RpcError::SendError(TrySendError::Full(()))
            }
        }
    }
}

impl From<RecvError> for RpcError {
    fn from(e: RecvError) -> Self {
        RpcError::RecvError(e)
    }
}

pub trait Request: Sized {
    type Value;
    type Error;

    fn req_type(&self) -> RequestType;
}
