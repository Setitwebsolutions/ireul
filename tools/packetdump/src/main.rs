extern crate ogg;

use std::env;
use std::fs::File;
use std::io::{self, Read};
use ogg::OggTrackBuf;
use ogg::vorbis::VorbisPacket;

fn main() {
    let filename = env::args_os().nth(1).unwrap();
    let mut file = File::open(filename).unwrap();

    let mut ogg_buf = Vec::new();
    file.read_to_end(&mut ogg_buf).unwrap();
    drop(file);

    let ogg_track = OggTrackBuf::new(ogg_buf).unwrap();

    let id = VorbisPacket::find_identification(ogg_track.pages()).unwrap();
    println!("identification header = {:?}", id.identification_header().unwrap());
    println!("identification header bytes = {:?}", id.as_u8_slice());

    for (pgi, page) in ogg_track.pages().enumerate() {
        for (pkti, packet) in page.raw_packets().enumerate() {
            println!("page[{}].packets[{}] = {:?}", pgi, pkti, packet);
        }
    }
}
