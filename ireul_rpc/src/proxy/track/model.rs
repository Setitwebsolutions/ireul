use std::io;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use ::proto::{self, Serialize, Deserialize};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct Handle(pub u64);

impl Deserialize for Handle {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let val: u64 = try!(Deserialize::read(buf));
        Ok(Handle(val))
    }
}

impl Serialize for Handle {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(Serialize::write(&self.0, buf));
        Ok(())
    }
}

const TRACK_INFO_FIELD_COUNT: u32 = 7;

#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub handle: Handle,

    pub artist: String,
    pub album: String,
    pub title: String,

    pub sample_rate: u64,
    pub sample_count: u64,
    pub sample_position: u64,

    // pub metadata: Vec<(String, String)>,
}

impl Deserialize for TrackInfo {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        try!(proto::expect_type(buf, proto::TYPE_STRUCT));
        let field_count = try!(buf.read_u32::<BigEndian>());

        let mut handle: Option<Handle> = None;
        let mut artist: Option<String> = None;
        let mut album: Option<String> = None;
        let mut title: Option<String> = None;
        let mut sample_rate: Option<u64> = None;
        let mut sample_count: Option<u64> = None;
        let mut sample_position: Option<u64> = None;
        // let mut metadata: Option<Vec<(String, String)>> = None;

        for _ in 0..field_count {
            let field_name: String = try!(Deserialize::read(buf));
            match &field_name[..] {
                "handle" => {
                    handle = Some(try!(Deserialize::read(buf)));
                },
                "artist" => {
                    artist = Some(try!(Deserialize::read(buf)));
                },
                "album" => {
                    album = Some(try!(Deserialize::read(buf)));
                },
                "title" => {
                    title = Some(try!(Deserialize::read(buf)));
                },
                "sample_rate" => {
                    sample_rate = Some(try!(Deserialize::read(buf)));
                },
                "sample_count" => {
                    sample_count = Some(try!(Deserialize::read(buf)));
                },
                "sample_position" => {
                    sample_position = Some(try!(Deserialize::read(buf)));
                },
                _ => try!(proto::skip_entity(buf)),
            }
        }

        let handle = match handle {
            Some(handle) => handle,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: handle")),
        };
        let artist = match artist {
            Some(artist) => artist,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: artist")),
        };
        let album = match album {
            Some(album) => album,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: album")),
        };
        let title = match title {
            Some(title) => title,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: title")),
        };
        let sample_rate = match sample_rate {
            Some(sample_rate) => sample_rate,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: sample_rate")),
        };
        let sample_count = match sample_count {
            Some(sample_count) => sample_count,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: sample_count")),
        };
        let sample_position = match sample_position {
            Some(sample_position) => sample_position,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: sample_position")),
        };

        Ok(TrackInfo {
            handle: handle,
            artist: artist,
            album: album,
            title: title,
            sample_rate: sample_rate,
            sample_count: sample_count,
            sample_position: sample_position,
        })
    }
}

impl Serialize for TrackInfo {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(proto::TYPE_STRUCT));
        try!(buf.write_u32::<BigEndian>(TRACK_INFO_FIELD_COUNT));

        try!(Serialize::write("handle", buf));
        try!(Serialize::write(&self.handle, buf));

        try!(Serialize::write("artist", buf));
        try!(Serialize::write(&*self.artist, buf));

        try!(Serialize::write("album", buf));
        try!(Serialize::write(&*self.album, buf));

        try!(Serialize::write("title", buf));
        try!(Serialize::write(&*self.title, buf));

        try!(Serialize::write("sample_rate", buf));
        try!(Serialize::write(&self.sample_rate, buf));

        try!(Serialize::write("sample_count", buf));
        try!(Serialize::write(&self.sample_count, buf));

        try!(Serialize::write("sample_position", buf));
        try!(Serialize::write(&self.sample_position, buf));

        Ok(())
    }
}

const QUEUE_FIELD_COUNT: u32 = 2;

#[derive(Debug)]
pub struct Queue {
    // We'll just include the currently playing song
    // in here.
    pub upcoming: Vec<TrackInfo>,
    pub history: Vec<TrackInfo>,
}

impl Deserialize for Queue {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        try!(proto::expect_type(buf, proto::TYPE_STRUCT));
        let field_count = try!(buf.read_u32::<BigEndian>());

        let mut upcoming: Option<Vec<TrackInfo>> = None;
        let mut history: Option<Vec<TrackInfo>> = None;

        for _ in 0..field_count {
            let field_name: String = try!(Deserialize::read(buf));
            match &field_name[..] {
                "upcoming" => {
                    upcoming = Some(try!(Deserialize::read(buf)));
                },
                "history" => {
                    history = Some(try!(Deserialize::read(buf)));
                }
                _ => try!(proto::skip_entity(buf)),
            }
        }

        let upcoming = match upcoming {
            Some(upcoming) => upcoming,
            None => return Err(io::Error::new(io::ErrorKind::Other, "missing field: upcoming")),
        };
        let history = history.unwrap_or_else(Vec::new);

        Ok(Queue {
            upcoming: upcoming,
            history: history,
        })
    }
}

impl Serialize for Queue {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(proto::TYPE_STRUCT));
        try!(buf.write_u32::<BigEndian>(QUEUE_FIELD_COUNT));

        try!(Serialize::write("upcoming", buf));
        try!(Serialize::write(&self.upcoming[..], buf));

        try!(Serialize::write("history", buf));
        try!(Serialize::write(&self.history[..], buf));

        Ok(())
    }
}
