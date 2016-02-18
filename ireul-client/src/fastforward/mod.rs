use std::io::{self, Read, Write};
use std::ffi::OsString;
use std::net::TcpStream;

use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};

use ireul_rpc::proto;
use ireul_rpc::proxy::{
    RequestType,
    FastForwardRequest,
    FastForward,
    FastForwardResult,
};

use ::entrypoint::{Error as EntryPointError};

pub struct EntryPoint;

unsafe impl Sync for EntryPoint {}

impl ::entrypoint::EntryPoint for EntryPoint {
    fn main(&self, args: Vec<OsString>) -> Result<(), EntryPointError> {
        main(args)
    }

    fn print_usage(&self, args: &[OsString]) {
        print_usage(args)
    }
}

fn main(_args: Vec<OsString>) -> Result<(), EntryPointError> {
    let mut conn = TcpStream::connect("127.0.0.1:3001").unwrap();

    try!(conn.write_u8(0));
    try!(conn.write_u32::<BigEndian>(RequestType::FastForward.to_op_code()));

    let req = FastForwardRequest {
        kind: FastForward::TrackBoundary,
    };
    let buf = proto::serialize(&req).unwrap();
    try!(conn.write_u32::<BigEndian>(buf.len() as u32));
    try!(conn.write_all(&buf));

    let frame_length = try!(conn.read_u32::<BigEndian>());
    let mut resp_buf = Vec::new();
    {
        let mut limit_reader = Read::by_ref(&mut conn).take(frame_length as u64);
        try!(limit_reader.read_to_end(&mut resp_buf));
    }

    let mut frame = io::Cursor::new(resp_buf);
    let res: FastForwardResult = proto::deserialize(&mut frame).unwrap();
    println!("got response: {:?}", res);

    try!(conn.write_u8(0));
    try!(conn.write_u32::<BigEndian>(0));
    Ok(())
}

fn print_usage(args: &[OsString]) {
    println!("{} fast-forward", args[0].clone().into_string().ok().unwrap());
    println!("");
    println!("    Skips the currently-playing track");
    println!("");
}
