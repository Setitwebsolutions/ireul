use std::thread;
use std::sync::{Arc, Mutex};
use std::net::{TcpStream, TcpListener};
use std::io::{self, Read, Write};

use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian, ByteOrder};
use time::SteadyTime;

use ireul_rpc::proto;
use ireul_rpc::proxy::RequestType;

use ::core::Core;

pub fn start(core: Core) {
    let core = Arc::new(Mutex::new(core));

    let control = TcpListener::bind("0.0.0.0:3001").unwrap();

    let client_core = core.clone();
    thread::spawn(move || {
        client_acceptor(control, client_core.clone());
    });

    loop {
        let next_tick_deadline = {
            let mut exc_core = core.lock().unwrap();
            exc_core.tick()
        };

        let sleep_time = next_tick_deadline - SteadyTime::now();
        ::std::thread::sleep_ms(sleep_time.num_milliseconds() as u32);
    }
}

fn client_worker(mut stream: TcpStream, core: Arc<Mutex<Core>>) -> io::Result<()> {
    const BUFFER_SIZE_LIMIT: usize = 20 * 1 << 20;
    loop {
        let version = try!(stream.read_u8());

        if version != 0 {
            let err_msg = format!("invalid version: {}", version);
            return Err(io::Error::new(io::ErrorKind::Other, err_msg));
        }

        let op_code = try!(stream.read_u32::<BigEndian>());
        if op_code == 0 {
            info!("goodbye, client");
            return Ok(());
        }

        let req_type = try!(RequestType::from_op_code(op_code).map_err(|_| {
            let err_msg = format!("unknown op-code {:?}", op_code);
            io::Error::new(io::ErrorKind::Other, err_msg)
        }));

        let frame_length = try!(stream.read_u32::<BigEndian>()) as usize;
        if BUFFER_SIZE_LIMIT < frame_length {
            let err_msg = format!("datagram too large: {} bytes (limit is {})",
                frame_length, BUFFER_SIZE_LIMIT);
            return Err(io::Error::new(io::ErrorKind::Other, err_msg));
        }

        let mut req_buf = Vec::new();
        {
            let mut limit_reader = Read::by_ref(&mut stream).take(frame_length as u64);
            try!(limit_reader.read_to_end(&mut req_buf));
        }

        if req_buf.len() != frame_length {
            let err_msg = format!(
                "datagram truncated: got {} bytes, expected {}",
                req_buf.len(), frame_length);
            return Err(io::Error::new(io::ErrorKind::Other, err_msg));
        }

        let mut cursor = io::Cursor::new(req_buf);
        let response = match req_type {
            RequestType::EnqueueTrack => {
                let req = proto::deserialize(&mut cursor).unwrap();
                let resp = {
                    let mut exc_core = core.lock().unwrap();
                    exc_core.enqueue_track(req)
                };
                proto::serialize(&resp).unwrap()
            },
            RequestType::FastForward => {
                let req = proto::deserialize(&mut cursor).unwrap();
                let resp = {
                    let mut exc_core = core.lock().unwrap();
                    exc_core.fast_forward(req)
                };
                proto::serialize(&resp).unwrap()
            },
            RequestType::QueueStatus => {
                let req = proto::deserialize(&mut cursor).unwrap();
                let resp = {
                    let mut exc_core = core.lock().unwrap();
                    exc_core.queue_status(req)
                };
                proto::serialize(&resp).unwrap()
            },
            RequestType::ReplaceFallback => {
                let req = proto::deserialize(&mut cursor).unwrap();
                let resp = {
                    let mut exc_core = core.lock().unwrap();
                    exc_core.replace_fallback(req)
                };
                proto::serialize(&resp).unwrap()            }
        };
        try!(stream.write_u32::<BigEndian>(response.len() as u32));
        try!(stream.write_all(&response));
    }
}

fn client_acceptor(server: TcpListener, core: Arc<Mutex<Core>>) {
    for stream in server.incoming() {
        match stream {
            Ok(stream) => {
                let client_core = core.clone();
                thread::spawn(move || {
                    if let Err(err) = client_worker(stream, client_core) {
                        info!("client disconnected with error: {:?}", err);
                    }
                });
            },
            Err(err) => {
                info!("error accepting new client: {:?}", err);
            }
        }
    }
}
