use std::io::{self, Read, Write};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

pub const TYPE_ARRAY: u16 = 0x0000;
pub const TYPE_BLOB: u16 = 0x0002;
pub const TYPE_STRUCT: u16 = 0x0005;

pub const TYPE_VOID: u16 = 0x0080;
pub const TYPE_U16: u16 = 0x0081;
pub const TYPE_U32: u16 = 0x0082;
pub const TYPE_U64: u16 = 0x0083;
pub const TYPE_STRING: u16 = 0x0084;
pub const TYPE_RESULT_OK: u16 = 0x0085;
pub const TYPE_RESULT_ERR: u16 = 0x0086;
pub const TYPE_TUPLE: u16 = 0x0087;


pub trait Deserialize: Sized {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self>;
}

pub trait Serialize {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()>;
}

impl Serialize for [u8] {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        let length = self.len();
        if 0xFFFFFFFF < length {
            return Err(io::Error::new(io::ErrorKind::Other, "excessive length"));
        }

        try!(buf.write_u16::<BigEndian>(TYPE_BLOB));
        try!(buf.write_u32::<BigEndian>(length as u32));
        try!(buf.write_all(self));
        Ok(())
    }
}

impl<T> Serialize for [T] where T: Serialize {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        let length = self.len();
        if 0xFFFFFFFF < length {
            return Err(io::Error::new(io::ErrorKind::Other, "excessive length"));
        }

        try!(buf.write_u16::<BigEndian>(TYPE_ARRAY));
        try!(buf.write_u32::<BigEndian>(length as u32));
        for item in self.iter() {
            try!(Serialize::write(item, buf));
        }

        Ok(())
    }
}

impl Serialize for () {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(TYPE_VOID));
        Ok(())
    }
}

impl Serialize for u16 {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(TYPE_U16));
        try!(buf.write_u16::<BigEndian>(*self));
        Ok(())
    }
}


impl Serialize for u32 {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(TYPE_U32));
        try!(buf.write_u32::<BigEndian>(*self));
        Ok(())
    }
}

impl Serialize for u64 {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(TYPE_U64));
        try!(buf.write_u64::<BigEndian>(*self));
        Ok(())
    }
}

impl Serialize for str {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        let length = self.as_bytes().len();
        if 0xFFFFFFFF < length {
            return Err(io::Error::new(io::ErrorKind::Other, "excessive length"));
        }

        try!(buf.write_u16::<BigEndian>(TYPE_STRING));
        try!(buf.write_u32::<BigEndian>(length as u32));
        try!(buf.write_all(self.as_bytes()));
        Ok(())
    }
}

// TODO(sell): this shouldn't exist.
impl Serialize for (String, String) {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        try!(buf.write_u16::<BigEndian>(TYPE_TUPLE));
        try!(buf.write_u32::<BigEndian>(2));
        try!(Serialize::write(&self.0[..], buf));
        try!(Serialize::write(&self.1[..], buf));
        Ok(())
    }
}

impl<V, E> Serialize for Result<V, E> where V: Serialize, E: Serialize {
    fn write(&self, buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
        match *self {
            Ok(ref ok) => {
                try!(buf.write_u16::<BigEndian>(TYPE_RESULT_OK));
                try!(Serialize::write(ok, buf));
            },
            Err(ref err) => {
                try!(buf.write_u16::<BigEndian>(TYPE_RESULT_ERR));
                try!(Serialize::write(err, buf));
            }
        }
        Ok(())
    }
}

impl Deserialize for Vec<u8> {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_BLOB {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let length = try!(buf.read_u32::<BigEndian>());

        let mut out = Vec::with_capacity(length as usize);
        {
            let mut limit_reader = Read::by_ref(buf).take(length as u64);
            try!(limit_reader.read_to_end(&mut out));
        }

        Ok(out)
    }
}

// TODO(sell): this shouldn't exist.
impl Deserialize for (String, String) {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_TUPLE {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let length = try!(buf.read_u32::<BigEndian>());
        if length != 2 {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected length"));
        }

        let left = try!(Deserialize::read(buf));
        let right = try!(Deserialize::read(buf));
        Ok((left, right))
    }
}

impl<T> Deserialize for Vec<T> where T: Deserialize {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_ARRAY {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let length = try!(buf.read_u32::<BigEndian>());

        let mut out = Vec::with_capacity(length as usize);
        for _ in 0..length {
            out.push(try!(Deserialize::read(buf)));
        }
        Ok(out)
    }
}

impl Deserialize for () {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_VOID {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        Ok(())
    }
}

impl Deserialize for u16 {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_U16 {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let value = try!(buf.read_u16::<BigEndian>());
        Ok(value)
    }
}


impl Deserialize for u32 {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_U32 {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let value = try!(buf.read_u32::<BigEndian>());
        Ok(value)
    }
}


impl Deserialize for u64 {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_U64 {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let value = try!(buf.read_u64::<BigEndian>());
        Ok(value)
    }
}


impl Deserialize for String {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        if type_id != TYPE_STRING {
            return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
        }

        let length = try!(buf.read_u32::<BigEndian>());

        let mut out = Vec::with_capacity(length as usize);
        {
            let mut limit_reader = Read::by_ref(buf).take(length as u64);
            try!(limit_reader.read_to_end(&mut out));
        }

        String::from_utf8(out)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "invalid string"))
    }
}

impl<V, E> Deserialize for Result<V, E> where V: Deserialize, E: Deserialize {
    fn read(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<Self> {
        let type_id = try!(buf.read_u16::<BigEndian>());
        match type_id {
            TYPE_RESULT_OK => {
                let val: V = try!(Deserialize::read(buf));
                Ok(Ok(val))
            },
            TYPE_RESULT_ERR => {
                let val: E = try!(Deserialize::read(buf));
                Ok(Err(val))
            },
            _ => {
                Err(io::Error::new(io::ErrorKind::Other, format!(
                    "unexpected type {} (want {} or {})",
                    type_id, TYPE_RESULT_OK, TYPE_RESULT_ERR)))
            }
        }
    }
}


pub fn null_read<R>(buf: &mut R, len: u64) -> io::Result<()> where R: Read {
    for byte in buf.by_ref().take(len).bytes() {
        try!(byte);
    }
    Ok(())
}

pub fn skip_entity(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
    let type_id = try!(buf.read_u16::<BigEndian>());

    match type_id {
        TYPE_ARRAY => {
            let length = try!(buf.read_u32::<BigEndian>());
            for _ in 0..length {
                try!(skip_entity(buf));
            }
            Ok(())
        }
        TYPE_BLOB => {
            let length = try!(buf.read_u32::<BigEndian>());
            try!(null_read(buf, length as u64));
            Ok(())
        }
        TYPE_STRUCT => {
            let length = try!(buf.read_u32::<BigEndian>());
            for _ in 0..length {
                try!(skip_entity(buf));
                try!(skip_entity(buf));
            }
            Ok(())
        }
        TYPE_VOID => {
            Ok(())
        }
        TYPE_U16 => {
            try!(buf.read_u16::<BigEndian>());
            Ok(())
        }
        TYPE_U32 => {
            try!(buf.read_u32::<BigEndian>());
            Ok(())
        }
        TYPE_U64 => {
            try!(buf.read_u64::<BigEndian>());
            Ok(())
        }
        TYPE_STRING => {
            let length = try!(buf.read_u32::<BigEndian>());
            try!(null_read(buf, length as u64));
            Ok(())
        }
        TYPE_RESULT_OK => {
            try!(skip_entity(buf));
            Ok(())
        }
        TYPE_RESULT_ERR => {
            try!(skip_entity(buf));
            Ok(())
        }
        _ => {
            Err(io::Error::new(io::ErrorKind::Other, "unknown type"))
        }
    }
}

pub fn expect_type(buf: &mut io::Cursor<Vec<u8>>, type_id: u16) -> io::Result<()> {
    let got_type_id = try!(buf.read_u16::<BigEndian>());
    if got_type_id != type_id {
        return Err(io::Error::new(io::ErrorKind::Other, "unexpected type"));
    }
    Ok(())
}

pub fn write_empty_struct(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
    try!(buf.write_u16::<BigEndian>(TYPE_STRUCT));
    try!(buf.write_u32::<BigEndian>(0));
    Ok(())
}

pub fn read_empty_struct(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<()> {
    try!(expect_type(buf, TYPE_STRUCT));
    let field_count = try!(buf.read_u32::<BigEndian>());
    for _ in 0..field_count {
        let _: String = try!(Deserialize::read(buf));
        try!(skip_entity(buf));
    }

    Ok(())
}

pub fn deserialize<T>(buf: &mut io::Cursor<Vec<u8>>) -> io::Result<T>
    where T: Deserialize
{
    Deserialize::read(buf)
}

pub fn serialize<T>(item: &T) -> io::Result<Vec<u8>>
    where T: Serialize
{
    let mut buf = io::Cursor::new(Vec::new());
    try!(Serialize::write(item, &mut buf));
    Ok(buf.into_inner())
}
