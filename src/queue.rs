use std::mem;
use std::collections::{VecDeque, HashMap, HashSet};

use rand::{self, Rng, ChaChaRng};

use ogg::{OggTrack, OggTrackBuf};
use ogg::vorbis::{Comments, VorbisPacket};
use ireul_rpc::proxy::track::model::{self, Handle};


struct HandleAllocator<R> {
    rng: R,
    limit: usize,
    allocated: HashSet<u64>,
}

impl<R> HandleAllocator<R> where R: Rng {
    pub fn new(rng: R, limit: usize) -> HandleAllocator<R> {
        HandleAllocator {
            rng: rng,
            limit: limit,
            allocated: HashSet::new(),
        }
    }

    pub fn generate(&mut self) -> Result<Handle, ()> {
        if self.limit <= self.allocated.len() {
            return Err(());
        }

        let new_handle;
        loop {
            let foo = self.rng.next_u64();
            if !self.allocated.contains(&foo) {
                new_handle = foo;
                break;
            }
        }

        println!("EMITTING HANDLE {:#x}", new_handle);
        self.allocated.insert(new_handle);
        Ok(Handle(new_handle))
    }

    pub fn dispose(&mut self, handle: Handle) -> Result<(), Handle> {
        match self.allocated.remove(&handle.0) {
            true => Ok(()),
            false => Err(handle),
        }
    }
}

#[derive(Clone)]
pub struct Track {
    handle: Handle,
    data: OggTrackBuf,
    comments: Comments,

    artist: String,
    album: String,
    title: String,

    sample_rate: u64,
    sample_count: u64,
}

impl Track {
    pub fn from_ogg_track(handle: Handle, ogg: OggTrackBuf) -> Track {
        use std::ascii::AsciiExt;

        let id_header = match VorbisPacket::find_identification(ogg.pages()) {
            Ok(header) => header.identification_header(),
            Err(_) => None
        }.expect("Invalid OggTrackBuf");

        let comments = match VorbisPacket::find_comments(ogg.pages()) {
            Ok(header) => header.comments(),
            Err(_) => None
        }.expect("Invalid OggTrackBuf");

        let mut sample_count = 0;
        for page in ogg.pages() {
            let page_pos = page.position();
            if sample_count < page_pos {
                sample_count = page_pos;
            }
        }

        let mut artist: Option<String> = None;
        let mut album: Option<String> = None;
        let mut title: Option<String> = None;

        for &(ref key, ref val) in comments.comments.iter() {
            if key.eq_ignore_ascii_case("ARTIST") {
                artist = Some(val.clone());
            }
            if key.eq_ignore_ascii_case("ALBUM") {
                album = Some(val.clone());
            }
            if key.eq_ignore_ascii_case("TITLE") {
                title = Some(val.clone());
            }
        }

        Track {
            handle: handle,
            data: ogg,
            comments: comments,

            artist: artist.unwrap_or_else(|| "".to_string()),
            album: album.unwrap_or_else(|| "".to_string()),
            title: title.unwrap_or_else(|| "".to_string()),

            sample_rate: id_header.audio_sample_rate as u64,
            sample_count: sample_count,
        }
    }

    pub fn into_inner(self) -> OggTrackBuf {
        self.data
    }

    pub fn get_track_info(&self) -> model::TrackInfo {
        model::TrackInfo {
            handle: self.handle,

            artist: self.artist.clone(),
            album: self.album.clone(),
            title: self.title.clone(),

            sample_rate: self.sample_rate,
            sample_count: self.sample_count,
            sample_position: 0,
        }
    }
}

pub struct PlayQueue {
    halloc: HandleAllocator<ChaChaRng>,
    items: VecDeque<Track>,
    history: VecDeque<model::TrackInfo>,
}

impl PlayQueue {
    pub fn new(limit: usize) -> PlayQueue {
        let rng: ChaChaRng = rand::thread_rng().gen();
        PlayQueue {
            halloc: HandleAllocator::new(rng, limit),
            items: VecDeque::new(),
            history: VecDeque::new(),
        }
    }

    #[allow(dead_code)] // queue manip stub
    pub fn reorder(&mut self, handle_ord: &[Handle]) {
        let old_items: VecDeque<Track> = mem::replace(&mut self.items, VecDeque::new());

        let mut map: HashMap<Handle, Track> = old_items.into_iter().map(|pq| (pq.handle, pq)).collect();

        for handle in handle_ord.iter() {
            match map.remove(handle) {
                Some(item) => {
                    self.items.push_back(item);
                },
                None => (),
            }
        }
    }

    #[allow(dead_code)] // queue manip stub
    pub fn remove_by_handle(&mut self, handle: Handle) {
        let old_items: VecDeque<Track> = mem::replace(&mut self.items, VecDeque::new());
        for item in old_items.into_iter() {
            if item.handle != handle {
                self.items.push_back(item);
            }
        }
    }

    pub fn add_track(&mut self, track: &OggTrack) -> Result<Handle, PlayQueueError> {
        let handle = try!(self.halloc.generate()
                .map_err(|()| PlayQueueError::Full));

        self.items.push_back(Track::from_ogg_track(handle, track.to_owned()));
        Ok(handle)
    }

    pub fn pop_track(&mut self) -> Option<Track> {
        match self.items.pop_front() {
            Some(track) => {
                self.add_history(track.get_track_info());
                Some(track)
            },
            None => None,
        }
    }

    pub fn track_infos(&self) -> Vec<model::TrackInfo> {
        self.items.iter()
            .map(Track::get_track_info)
            .collect()
    }

    fn add_history(&mut self, tinfo: model::TrackInfo) {
        self.history.push_front(tinfo);
        while 10 < self.history.len() {
            let track = self.history.pop_back().unwrap();
            self.halloc.dispose(track.handle).unwrap();
        }
    }

    pub fn get_history(&self) -> Vec<model::TrackInfo> {
        self.history.iter().skip(1).cloned().collect()

    }
}

pub enum PlayQueueError {
    Full,
}
