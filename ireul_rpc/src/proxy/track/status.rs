use std::io;

use super::super::{RequestType, Request};
use super::model::{Queue};

use ::proto::{self, Deserialize, Serialize};


/// Skips to the end of the currently playing track
#[derive(Debug, Clone)]
pub struct StatusRequest;

impl Deserialize for StatusRequest {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        try!(proto::read_empty_struct(buf));
        Ok(StatusRequest)
    }
}

impl Serialize for StatusRequest {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(proto::write_empty_struct(buf));
        Ok(())
    }
}

impl Request for StatusRequest {
    type Value = ();
    type Error = StatusError;

    fn req_type(&self) -> RequestType {
        RequestType::FastForward
    }
}

pub type StatusResult = Result<Queue, StatusError>;

#[derive(Debug, Clone)]
pub struct StatusError;

impl Deserialize for StatusError {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        try!(proto::read_empty_struct(buf));
        Ok(StatusError)
    }
}

impl Serialize for StatusError {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(proto::write_empty_struct(buf));
        Ok(())
    }
}
