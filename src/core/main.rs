#[macro_use]
extern crate log;

extern crate byteorder;
extern crate env_logger;
extern crate ireul_interface;
extern crate ogg;
extern crate ogg_clock;
extern crate rand;
extern crate rustc_serialize;
extern crate toml;
extern crate url;

use std::thread;
use std::env;
use std::fmt;
use std::collections::HashSet;
use std::sync::mpsc::{self};
use std::net::{TcpStream, TcpListener};
use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::fs::File;

use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian, ByteOrder};

use ogg::{OggTrack, OggTrackBuf, OggPage, OggPageBuf};
use ogg::vorbis::VorbisHeader;
use ogg_clock::OggClock;

use ireul_interface::proto;
use ireul_interface::proxy::track::model::{self, Handle};
use ireul_interface::proxy::track::{
    StatusRequest,
    StatusResult,
};
use ireul_interface::proxy::{
    RequestWrapper,
    RequestType,
    BinderError,
    EnqueueTrackRequest,
    EnqueueTrackError,
    EnqueueTrackResult,
    FastForward,
    FastForwardRequest,
    FastForwardResult,
};

mod queue;
mod icecastwriter;

use queue::{PlayQueue, PlayQueueError};
use icecastwriter::{
    IceCastWriter,
    IceCastWriterOptions,
};

#[derive(RustcDecodable, Debug)]
struct MetadataConfig {
    name: Option<String>,
    description: Option<String>,
    url: Option<String>,
    genre: Option<String>,
}

#[derive(RustcDecodable, Debug)]
struct Config {
    icecast_url: String,
    metadata: Option<MetadataConfig>,
}

impl Config {
    fn icecast_writer_opts(&self) -> Result<IceCastWriterOptions, String> {
        let url = try!(url::Url::parse(&self.icecast_url)
            .map_err(|err| format!("Malformed URL: {:?}", err)));

        let mut opts = try!(IceCastWriterOptions::from_url(&url)
            .map_err(|err| format!("Unacceptable URL: {:?}", err)));

        if let Some(ref metadata) = self.metadata {
            if let Some(ref name) = metadata.name {
                opts.set_name(name);
            }
            if let Some(ref description) = metadata.description {
                opts.set_description(description);
            }
            if let Some(ref url) = metadata.url {
                opts.set_url(url);
            }
            if let Some(ref genre) = metadata.genre {
                opts.set_genre(genre);
            }
        }

        Ok(opts)
    }
}

fn main() {
    env_logger::init().unwrap();

    let config_file = env::args_os().nth(1).unwrap();
    let config: Config = {
        let mut reader = File::open(&config_file).expect("failed to open config file");
        let mut config_buf = String::new();
        reader.read_to_string(&mut config_buf).expect("failed to read config");
        toml::decode_str(&config_buf).expect("invalid config file")
    };
    let icecast_options = config.icecast_writer_opts().unwrap();

    let connector = IceCastWriter::new(icecast_options).unwrap();
    let mut file = File::open("howbigisthis.ogg").unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    let offline_track = OggTrackBuf::new(buffer).unwrap();

    let play_queue = PlayQueue::new(50);

    let output_manager = OutputManager {
        connector: connector,
        cur_serial: 0,
        cur_sequence: 0,
        // position: 0,
        clock: OggClock::new(48000),
        playing_offline: false,
        buffer: VecDeque::new(),
        play_queue: PlayQueue::new(10),
        offline_track: queue::Track::from_ogg_track(Handle(0), offline_track),
        playing: None,
    };

    let control = TcpListener::bind("0.0.0.0:3001").unwrap();
    let mut core = Core::new(control, output_manager).unwrap();
    loop {
        core.tick();

    }
}

fn validate_positions(track: &OggTrack) -> Result<(), ()> {
    let mut current = 0;
    let mut is_first = true;

    for page in track.pages() {
        let position = page.position();

        if is_first {
            is_first = false;

            if position != 0 {
                return Err(());
            }
        }

        if position < current {
            return Err(());
        }
        current = position;
    }

    Ok(())
}

fn validate_comment_section(track: &OggTrack) -> Result<(), ()> {
    let _ = try!(VorbisHeader::find_comments(track.pages()));
    Ok(())
}

fn check_sample_rate(req: u32, track: &OggTrack) -> Result<(), ()> {
    let packet = try!(VorbisHeader::find_identification(track.pages()));

    // find_identification will always find a packet with an identification_header
    let id_header = packet.identification_header().unwrap();

    if id_header.audio_sample_rate == req {
        Ok(())
    } else {
        Err(())
    }
}


fn update_serial(serial: u32, track: &mut OggTrack) {
    for page in track.pages_mut() {
        page.set_serial(serial);
    }
}

fn update_positions(start_pos: u64, track: &mut OggTrack) {
    for page in track.pages_mut() {
        let old_pos = page.position();
        page.set_position(start_pos + old_pos);
    }
}

fn final_position(track: &OggTrack) -> Option<u64> {
    let mut position = None;
    for page in track.pages() {
        position = Some(page.position());
    }
    position
}

struct Core {
    output: OutputManager,
    proxy_rx: mpsc::Receiver<RequestWrapper>,
}

impl Core {
    fn new(control: TcpListener, om: OutputManager) -> io::Result<Core> {
        let (tx, rx) = mpsc::sync_channel(5);

        let proxy_tx_client = tx.clone();
        thread::spawn(move || {
            client_acceptor(control, proxy_tx_client);
        });

        Ok(Core {
            output: om,
            proxy_rx: rx,
        })
    }

    fn enqueue_track(&mut self, req: EnqueueTrackRequest) -> EnqueueTrackResult {
        let EnqueueTrackRequest { mut track } = req;
        {
            let mut pages = 0;
            let mut samples = 0;
            for page in track.pages() {
                pages += 1;
                samples = page.position();
            }

            info!("a client sent {} samples in {} pages", samples, pages);
        }
        if track.as_u8_slice().len() == 0 {
            return Err(EnqueueTrackError::InvalidTrack);
        }

        try!(validate_positions(&track)
            .map_err(|()| EnqueueTrackError::InvalidTrack));

        try!(validate_comment_section(&track)
            .map_err(|()| EnqueueTrackError::InvalidTrack));

        try!(check_sample_rate(self.output.clock.sample_rate(), &track)
            .map_err(|()| EnqueueTrackError::BadSampleRate));

        let handle = self.output.play_queue.add_track(track.as_ref())
            .map_err(|err| match err {
                PlayQueueError::Full => EnqueueTrackError::Full,
            });

        if self.output.playing_offline {
            self.output.fast_forward_track_boundary();
        }

        handle
    }

    fn fast_forward(&mut self, req: FastForwardRequest) -> FastForwardResult {
        try!(self.output.fast_forward(req.kind));
        Ok(())
    }

    fn queue_status(&mut self, _req: StatusRequest) -> StatusResult {
        Ok(model::Queue {
            upcoming: self.output.get_track_infos(),
        })
    }

    fn handle_command(&mut self, req_wr: RequestWrapper) {
        let mut binder = CoreBinder { core: self };
        binder.handle_command(req_wr)
    }

    fn tick(&mut self) {
        loop {
            match self.proxy_rx.try_recv() {
                Ok(cmd) => self.handle_command(cmd),
                Err(err) => break,
            }
        }
        self.output.copy_page();
    }
}

fn client_worker(mut stream: TcpStream, chan: mpsc::SyncSender<RequestWrapper>) -> io::Result<()> {
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

        let (resp_tx, resp_rx) = mpsc::sync_channel(1);
        chan.send(RequestWrapper {
            response_queue: resp_tx,
            req_type: req_type,
            req_buf: req_buf,
        }).unwrap();

        let response = resp_rx.recv().unwrap();
        try!(stream.write_u32::<BigEndian>(response.len() as u32));
        try!(stream.write_all(&response));
    }
}

fn client_acceptor(server: TcpListener, chan: mpsc::SyncSender<RequestWrapper>) {
    for stream in server.incoming() {
        match stream {
            Ok(stream) => {
                let client_chan = chan.clone();
                thread::spawn(move || {
                    if let Err(err) = client_worker(stream, client_chan) {
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

struct CoreBinder<'a> {
    core: &'a mut Core,
}

impl<'a> CoreBinder<'a> {
    fn handle_command(&mut self, req_wr: RequestWrapper) {
        info!("CoreBinder::handle_command");
        let RequestWrapper {
            response_queue: response_queue,
            req_type: req_type,
            req_buf: req_buf,
        } = req_wr;
        let response = match req_type {
            RequestType::EnqueueTrack => {
                self.enqueue_track(&req_buf)
            },
            RequestType::FastForward => {
                self.fast_forward(&req_buf)
            },
            RequestType::QueueStatus => {
                self.queue_status(&req_buf)
            }
        };
        response_queue.send(response).unwrap();
    }

    fn enqueue_track(&mut self, req: &[u8]) -> Vec<u8> {
        info!("CoreBinder::enqueue_track");
        let mut cursor = io::Cursor::new(req.to_vec());
        let req: EnqueueTrackRequest = proto::deserialize(&mut cursor).unwrap();
        let resp = self.core.enqueue_track(req);
        proto::serialize(&resp).unwrap()
    }

    fn fast_forward(&mut self, req: &[u8]) -> Vec<u8> {
        info!("CoreBinder::fast_forward");
        let mut cursor = io::Cursor::new(req.to_vec());
        let req: FastForwardRequest = proto::deserialize(&mut cursor).unwrap();
        let resp = self.core.fast_forward(req);
        proto::serialize(&resp).unwrap()
    }

    fn queue_status(&mut self, req: &[u8]) -> Vec<u8> {
        info!("CoreBinder::queue_status");
        let mut cursor = io::Cursor::new(req.to_vec());
        let req: StatusRequest = proto::deserialize(&mut cursor).unwrap();
        let resp = self.core.queue_status(req);
        proto::serialize(&resp).unwrap()
    }
}

/// Connects to IceCast and holds references to streamable content.
struct OutputManager {
    connector: IceCastWriter,
    cur_serial: u32,
    cur_sequence: u32,
    clock: OggClock,

    playing_offline: bool,
    buffer: VecDeque<OggPageBuf>,
    play_queue: PlayQueue,
    offline_track: queue::Track,
    playing: Option<model::TrackInfo>,
}

impl OutputManager {
    fn fill_buffer(&mut self) {
        let track: queue::Track = match self.play_queue.pop_track() {
            Some(track) => {
                self.playing_offline = false;
                self.playing = Some(track.get_track_info());
                track
            },
            None => {
                self.playing_offline = true;
                self.playing = None;
                self.offline_track.clone()
            }
        };
        let mut track = track.into_inner();
        // not sure why we as_mut instead of just using &mut track
        update_serial(self.cur_serial, track.as_mut());
        self.cur_serial = self.cur_serial.wrapping_add(0);

        self.buffer.extend(track.pages().map(|x| x.to_owned()));
    }

    fn get_next_page(&mut self) -> OggPageBuf {
        if self.buffer.is_empty() {
            self.fill_buffer();
        }
        self.buffer.pop_front().unwrap()
    }

    fn fast_forward_track_boundary(&mut self) -> FastForwardResult {
        loop {
            let page = self.get_next_page();
            debug!("checking page...");
            if page_starts_track(page.as_ref()) {
                debug!("checking page... found a start");
                self.buffer.push_front(page);
                break;
            }
        }
        Ok(())
    }

    fn fast_forward(&mut self, kind: FastForward) -> FastForwardResult {
        match kind {
            FastForward::TrackBoundary => {
                self.fast_forward_track_boundary()
            }
        }
    }

    fn copy_page(&mut self) {
        let page = self.get_next_page();
        self.clock.wait(&page).unwrap();
        self.connector.send_ogg_page(&page).unwrap();

        if let Some(playing) = self.playing.as_mut() {
            playing.sample_position = page.position();
        }

        debug!("copied page :: granule_pos = {:?}; serial = {:?}; sequence = {:?}",
            page.position(),
            page.serial(),
            page.sequence());
    }

    fn get_track_infos(&self) -> Vec<model::TrackInfo> {
        let mut out: Vec<model::TrackInfo> = Vec::new();
        if let Some(ref playing) = self.playing {
            out.push(playing.clone());
        }
        out.extend(self.play_queue.track_infos().into_iter());
        out
    }
}

fn page_starts_track(page: &OggPage) -> bool {
    page.body().starts_with(b"\x01vorbis")
}
