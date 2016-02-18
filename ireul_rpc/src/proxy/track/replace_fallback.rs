use std::io;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use ogg::{OggTrackBuf};

use super::super::{RequestType, Request};
use super::model::{Queue};

use ::proto::{self, Deserialize, Serialize};


/// Skips to the end of the currently playing track
#[derive(Clone)]
pub struct ReplaceFallbackRequest {
    pub track: OggTrackBuf,
    pub metadata: Option<Vec<(String, String)>>,
}

impl Deserialize for ReplaceFallbackRequest {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        try!(proto::expect_type(buf, proto::TYPE_STRUCT));
        let field_count = try!(buf.read_u32::<BigEndian>());

        let mut track: Option<Vec<u8>> = None;
        let mut metadata: Option<Vec<(String, String)>> = None;

        for _ in 0..field_count {
            let field_name: String = try!(Deserialize::read(buf));
            match &field_name[..] {
                "track" => {
                    track = Some(try!(Deserialize::read(buf)));
                },
                "metadata" => {
                    metadata = Some(try!(Deserialize::read(buf)));
                }
                _ => try!(proto::skip_entity(buf)),
            }
        }

        let track = match track {
            Some(track) => track,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: track")),
        };

        let track = try!(OggTrackBuf::new(track)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "invalid ogg")));

        Ok(ReplaceFallbackRequest {
            track: track,
            metadata: metadata,
        })
    }
}

impl Serialize for ReplaceFallbackRequest {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(proto::TYPE_STRUCT));

        let length = if self.metadata.is_some() { 2 } else { 1 };
        try!(buf.write_u32::<BigEndian>(length));

        try!(Serialize::write("track", buf));
        try!(Serialize::write(self.track.as_u8_slice(), buf));

        if let Some(ref metadata) = self.metadata {
            try!(Serialize::write("metadata", buf));
            try!(Serialize::write(&metadata[..], buf));
        }

        Ok(())
    }
}

impl Request for ReplaceFallbackRequest {
    type Value = ();
    type Error = ReplaceFallbackError;

    fn req_type(&self) -> RequestType {
        RequestType::ReplaceFallback
    }
}

pub type ReplaceFallbackResult = Result<(), ReplaceFallbackError>;

#[derive(Debug, Clone)]
pub enum ReplaceFallbackError {
    InvalidTrack = 1,

    BadSampleRate = 2,

    Full = 3,
}

impl ReplaceFallbackError {
    pub fn to_u32(&self) -> u32 {
        self.clone() as u32
    }

    pub fn from_u32(val: u32) -> Option<ReplaceFallbackError> {
        match val {
            1 => Some(ReplaceFallbackError::InvalidTrack),
            2 => Some(ReplaceFallbackError::BadSampleRate),
            3 => Some(ReplaceFallbackError::Full),
            _ => None
        }
    }
}

impl Deserialize for ReplaceFallbackError {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let num: u32 = try!(Deserialize::read(buf));
        ReplaceFallbackError::from_u32(num)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "unexpected ReplaceFallbackError value")
            })
    }
}

impl Serialize for ReplaceFallbackError {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(Serialize::write(&self.to_u32(), buf));
        Ok(())
    }
}
