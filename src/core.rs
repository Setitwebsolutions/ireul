use std::mem;
use std::collections::VecDeque;

use time::SteadyTime;

use ogg::{OggTrack, OggTrackBuf, OggPageBuf, OggBuilder};
use ogg::vorbis::{VorbisPacket, VorbisPacketBuf, Comments as VorbisComments};
use ogg_clock::OggClock;

use ireul_rpc::proxy::track::model::{self, Handle};
use ireul_rpc::proxy::track::{
    StatusRequest,
    StatusResult,
};

use ireul_rpc::proxy::{
    EnqueueTrackRequest,
    EnqueueTrackError,
    EnqueueTrackResult,
    FastForward,
    FastForwardRequest,
    FastForwardResult,
    ReplaceFallbackRequest,
    ReplaceFallbackResult,
    ReplaceFallbackError,
};

use queue::{self, PlayQueue, PlayQueueError};
use icecastwriter::IceCastWriter;


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
    let _ = try!(VorbisPacket::find_comments(track.pages()));
    Ok(())
}

fn check_sample_rate(req: u32, track: &OggTrack) -> Result<(), ()> {
    let packet = try!(VorbisPacket::find_identification(track.pages()));

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

/// Connects to IceCast and holds references to streamable content.
pub struct Core {
    pub connector: IceCastWriter,
    pub cur_serial: u32,
    pub clock: OggClock,

    pub playing_offline: bool,
    pub buffer: VecDeque<OggPageBuf>,

    pub prev_ogg_granule_pos: u64,
    pub prev_ogg_serial: u32,
    pub prev_ogg_sequence: u32,

    pub play_queue: PlayQueue,
    pub offline_track: queue::Track,
    pub playing: Option<model::TrackInfo>,
}

impl Core {
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
        self.cur_serial = self.cur_serial.wrapping_add(1);

        self.buffer.extend(track.pages().map(|x| x.to_owned()));
    }

    fn get_next_page(&mut self) -> OggPageBuf {
        if self.buffer.is_empty() {
            self.fill_buffer();
        }
        self.buffer.pop_front().unwrap()
    }

    pub fn fast_forward_track_boundary(&mut self) -> FastForwardResult {
        let old_buffer = mem::replace(&mut self.buffer, VecDeque::new());

        let mut page_iter = old_buffer.into_iter();

        while let Some(page) = page_iter.next() {
            debug!("checking buffer for non-continued page...");
            if page.as_ref().continued() {
                debug!("checking buffer for non-continued page... continued; kept");
                self.buffer.push_back(page);
            } else {
                debug!("checking buffer for non-continued page...found page-aligned packet!");
                break;
            }
        }
        while let Some(mut page) = page_iter.next() {
            // debug!("checking page for EOS...");
            if page.as_ref().eos() {
                {
                    let mut tx = page.as_mut().begin();
                    tx.set_position(self.prev_ogg_granule_pos);
                    tx.set_serial(self.prev_ogg_serial);
                    tx.set_sequence(self.prev_ogg_sequence + 1);
                }
                debug!("checking page for EOS... found it!");
                self.buffer.push_back(page);
                break;
            }
        }

        self.buffer.extend(page_iter);
        Ok(())
    }

    // **
    pub fn enqueue_track(&mut self, req: EnqueueTrackRequest) -> EnqueueTrackResult {
        let EnqueueTrackRequest { track, metadata } = req;
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

        try!(check_sample_rate(self.clock.sample_rate(), &track)
            .map_err(|()| EnqueueTrackError::BadSampleRate));

        let track = rewrite_comments(track.as_ref(), |comments| {
            comments.vendor = "Ireul Core".to_string();
            if let Some(ref metadata) = metadata {
                comments.comments.clear();
                comments.comments.extend(metadata.iter().cloned());
            }
        });

        let handle = self.play_queue.add_track(track.as_ref())
            .map_err(|err| match err {
                PlayQueueError::Full => EnqueueTrackError::Full,
            });

        if self.playing_offline {
            self.fast_forward_track_boundary().unwrap();
        }

        handle
    }

    pub fn fast_forward(&mut self, req: FastForwardRequest) -> FastForwardResult {
        match req.kind {
            FastForward::TrackBoundary => {
                try!(self.fast_forward_track_boundary());
                Ok(())
            }
        }
    }

    pub fn queue_status(&mut self, _req: StatusRequest) -> StatusResult {
        let mut upcoming: Vec<model::TrackInfo> = Vec::new();
        if let Some(ref playing) = self.playing {
            upcoming.push(playing.clone());
        }
        upcoming.extend(self.play_queue.track_infos().into_iter());

        Ok(model::Queue {
            upcoming: upcoming,
            history: self.play_queue.get_history(),
        })
    }

    pub fn replace_fallback(&mut self, req: ReplaceFallbackRequest) -> ReplaceFallbackResult {
        let ReplaceFallbackRequest { track, metadata } = req;
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
            return Err(ReplaceFallbackError::InvalidTrack);
        }

        try!(validate_positions(&track)
            .map_err(|()| ReplaceFallbackError::InvalidTrack));

        try!(validate_comment_section(&track)
            .map_err(|()| ReplaceFallbackError::InvalidTrack));

        try!(check_sample_rate(self.clock.sample_rate(), &track)
            .map_err(|()| ReplaceFallbackError::BadSampleRate));

        let track = rewrite_comments(track.as_ref(), |comments| {
            comments.vendor = "Ireul Core".to_string();
            if let Some(ref metadata) = metadata {
                comments.comments.clear();
                comments.comments.extend(metadata.iter().cloned());
            }
        });

        self.offline_track = queue::Track::from_ogg_track(Handle(0), track);

        Ok(())
    }

    // copy a page and tells us up to when we have no work to do
    pub fn tick(&mut self) -> SteadyTime {
        let page = self.get_next_page();

        self.prev_ogg_granule_pos = page.position();
        self.prev_ogg_serial = page.serial();
        self.prev_ogg_sequence = page.sequence();

        if let Err(_err) = self.connector.send_ogg_page(&page) {
            //
        }

        if let Some(playing) = self.playing.as_mut() {
            playing.sample_position = page.position();
        }

        debug!("copied page :: granule_pos = {:?}; serial = {:?}; sequence = {:?}; bos = {:?}; eos = {:?}",
            page.position(),
            page.serial(),
            page.sequence(),
            page.bos(),
            page.eos());

        let vhdr = page.raw_packets().nth(0)
            .and_then(|packet| VorbisPacket::new(packet).ok())
            .and_then(|vhdr| vhdr.identification_header());

        if let Some(vhdr) = vhdr {
            debug!("            :: {:?}", vhdr);
        }

        SteadyTime::now() + self.clock.wait_duration(&page)
    }
}

fn rewrite_comments<F>(track: &OggTrack, func: F) -> OggTrackBuf
    where F: Fn(&mut VorbisComments) -> ()
{
    let mut track_rw: Vec<u8> = Vec::new();

    for page in track.pages() {
        // determine if we have a comment packet
        let mut have_comment = false;
        for packet in page.raw_packets() {
            if let Ok(vpkt) = VorbisPacket::new(packet) {
                if vpkt.comments().is_some() {
                    have_comment = true;
                }
            }
        }

        // fast-path: no comment
        if !have_comment {
            track_rw.extend(page.as_u8_slice());
            continue;
        }

        let mut builder = OggBuilder::new();
        for packet in page.raw_packets() {
            let mut emitted = false;
            if let Ok(vpkt) = VorbisPacket::new(packet) {
                if let Some(mut comments) = vpkt.comments() {
                    func(&mut comments);

                    let new_vpkt = VorbisPacketBuf::build_comment_packet(&comments);
                    builder.add_packet(new_vpkt.as_u8_slice());
                    emitted = true;
                }
            }
            if !emitted {
                println!("adding packet: {:?}", packet);
                builder.add_packet(packet);
            }
        }

        let mut new_page = builder.build().unwrap();
        {
            let mut tx = new_page.as_mut().begin();
            tx.set_position(page.position());
            tx.set_serial(page.serial());
            tx.set_sequence(page.sequence());
            tx.set_continued(page.continued());
            tx.set_bos(page.bos());
            tx.set_eos(page.eos());
        }

        track_rw.extend(new_page.as_u8_slice());
    }

    OggTrackBuf::new(track_rw).unwrap()
}
