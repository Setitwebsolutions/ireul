#[macro_use]
extern crate log;

extern crate byteorder;
extern crate env_logger;
extern crate ireul_rpc;
extern crate ogg;
extern crate ogg_clock;
extern crate rand;
extern crate rustc_serialize;
extern crate toml;
extern crate url;
extern crate time;

use std::env;
use std::collections::VecDeque;
use std::io::Read;
use std::fs::File;

use ogg::{OggTrack, OggTrackBuf};
use ogg_clock::OggClock;

use ireul_rpc::proxy::track::model::Handle;

mod queue;
mod icecastwriter;
mod core;
mod eloop;

use queue::PlayQueue;
use icecastwriter::{
    IceCastWriter,
    IceCastWriterOptions,
};

const DEAD_AIR: &'static [u8] = include_bytes!("deadair.ogg");

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
    fallback_track: Option<String>,
}

impl Config {
    fn icecast_url(&self) -> Result<url::Url, String> {
        let url = try!(url::Url::parse(&self.icecast_url)
            .map_err(|err| format!("Malformed URL: {:?}", err)));
        Ok(url)
    }

    fn icecast_writer_opts(&self) -> Result<IceCastWriterOptions, String> {
        let mut opts = IceCastWriterOptions::default();
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

    let icecast_url = config.icecast_url().unwrap();
    let icecast_options = config.icecast_writer_opts().unwrap();
    let connector = IceCastWriter::with_options(&icecast_url, icecast_options).unwrap();

    let mut offline_track = OggTrack::new(DEAD_AIR).unwrap().to_owned();

    if let Some(ref filename) = config.fallback_track {
        let mut file = File::open(filename).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        offline_track = OggTrackBuf::new(buffer).unwrap();
    }


    let core = core::Core {
        connector: connector,
        cur_serial: 0,
        clock: OggClock::new(48000),
        playing_offline: false,
        buffer: VecDeque::new(),

        prev_ogg_granule_pos: 0,
        prev_ogg_serial: 0,
        prev_ogg_sequence: 0,

        play_queue: PlayQueue::new(100),
        offline_track: queue::Track::from_ogg_track(Handle(0), offline_track),
        playing: None,
    };

    eloop::control::start(core);
}
