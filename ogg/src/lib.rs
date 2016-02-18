extern crate byteorder;

use std::mem;
use std::ops;
use std::borrow::{Borrow, BorrowMut, ToOwned};
use std::marker;
use std::io::{Cursor, BufRead};
use std::marker::PhantomData;
use std::borrow::Cow;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt, ByteOrder};
use byteorder::Error as ByteOrderError;

mod reader;
mod slice;
pub mod vorbis;
mod crc;

use slice::Slice;

const OGG_PAGE_CAPTURE: &'static [u8] = b"OggS";
const POSITION_OFFSET: usize = 6;
const SERIAL_OFFSET: usize = 14;
const SEQUENCE_OFFSET: usize = 18;
const CHECKSUM_OFFSET: usize = 22;

#[derive(Debug)]
pub enum OggPageCheckError {
    TooShort,
    BadCapture,
    BadVersion,
    BadCrc,
}

#[derive(Clone)]
pub struct OggTrackBuf {
    inner: Vec<u8>,
}

pub struct OggTrack {
    inner: Slice,
}

impl ops::Deref for OggTrackBuf {
    type Target = OggTrack;

    fn deref<'a>(&'a self) -> &'a OggTrack {
        OggTrack::from_u8_slice_unchecked(&self.inner)
    }
}

impl Borrow<OggTrack> for OggTrackBuf {
    fn borrow(&self) -> &OggTrack {
        OggTrack::from_u8_slice_unchecked(&self.inner)
    }
}

impl AsRef<OggTrack> for OggTrackBuf {
    fn as_ref(&self) -> &OggTrack {
        OggTrack::from_u8_slice_unchecked(&self.inner)
    }
}

impl AsMut<OggTrack> for OggTrackBuf {
    fn as_mut(&mut self) -> &mut OggTrack {
        OggTrack::from_u8_slice_unchecked_mut(&mut self.inner)
    }
}

impl BorrowMut<OggTrack> for OggTrackBuf {
    fn borrow_mut(&mut self) -> &mut OggTrack {
        OggTrack::from_u8_slice_unchecked_mut(&mut self.inner)
    }
}

impl ToOwned for OggTrack {
    type Owned = OggTrackBuf;

    fn to_owned(&self) -> OggTrackBuf {
        OggTrackBuf { inner: self.inner.to_owned() }
    }
}

impl OggTrackBuf {
    pub fn new(buf: Vec<u8>) -> Result<OggTrackBuf, OggPageCheckError> {
        try!(OggTrack::new(&buf));
        Ok(OggTrackBuf { inner: buf })
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.inner
    }
}

impl OggTrack {
    pub fn new(buf: &[u8]) -> Result<&OggTrack, OggPageCheckError>  {
        let mut offset = 0;
        while offset < buf.len() {
            let page = try!(OggPage::new(&buf[offset..]));
            offset += page.as_u8_slice().len();
        }
        Ok(OggTrack::from_u8_slice_unchecked(buf))
    }

    /// The following function allows unchecked construction of a ogg track
    /// from a u8 slice.  This is private because it does not maintain
    /// the OggPage invariant.
    fn from_u8_slice_unchecked(s: &[u8]) -> &OggTrack {
        unsafe { mem::transmute(s) }
    }

    /// The following (private!) function allows unchecked construction of a
    /// mutable ogg page from a mutable u8 slice.  This is private because it
    /// does not maintain the OggPage invariant.
    fn from_u8_slice_unchecked_mut(s: &mut [u8]) -> &mut OggTrack {
        unsafe { mem::transmute(s) }
    }

    pub fn as_u8_slice(&self) -> &[u8] {
        &self.inner
    }

    /// Mutably borrow the underlying storage.  This is private because it
    /// does not maintain the OggPage invariant.
    fn as_u8_slice_mut(&mut self) -> &mut [u8] {
        unsafe { mem::transmute(self) }
    }

    pub fn pages(&self) -> TrackPageIter {
        TrackPageIter {
            data: self.as_u8_slice(),
            offset: 0,
        }
    }

    pub fn pages_mut(&mut self) -> TrackPageIterMut {
        let buffer = self.as_u8_slice_mut();
        let ptr = buffer.as_mut_ptr();
        let end = unsafe { ptr.offset(buffer.len() as isize) };
        TrackPageIterMut {
            ptr: ptr,
            end: end,
            _marker: marker::PhantomData,
        }
    }
}

pub struct TrackPageIter<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for TrackPageIter<'a> {
    type Item = &'a OggPage;

    fn next(&mut self) -> Option<&'a OggPage> {
        if self.data.len() == self.offset {
            return None;
        }
        let page = OggPage::new(&self.data[self.offset..]).unwrap();
        self.offset += page.as_u8_slice().len();
        Some(page)
    }
}

pub struct TrackPageIterMut<'a> {
    ptr: *mut u8,
    end: *mut u8,
    _marker: marker::PhantomData<&'a mut ()>,
}

impl<'a> Iterator for TrackPageIterMut<'a> {
    type Item = &'a mut OggPage;

    fn next(&mut self) -> Option<&'a mut OggPage> {
        if self.ptr == self.end {
            return None;
        }

        let length = self.end as usize - self.ptr as usize;
        let next_boundary = {
            let remaining_data = unsafe { std::slice::from_raw_parts(self.ptr, length) };
            let page_len = OggPage::new(remaining_data).unwrap().as_u8_slice().len();
            assert!(page_len <= length);
            page_len
        };

        let page_data;
        unsafe {
            page_data = std::slice::from_raw_parts_mut(self.ptr, next_boundary);
            self.ptr = self.ptr.offset(next_boundary as isize);
        }

        Some(OggPage::from_u8_slice_unchecked_mut(page_data))
    }
}

#[derive(Clone)]
pub struct OggPageBuf {
    inner: Vec<u8>,
}

pub struct OggPage {
    inner: Slice,
}

impl AsRef<OggPage> for OggPageBuf {
    fn as_ref(&self) -> &OggPage {
        OggPage::from_u8_slice_unchecked(&self.inner)
    }
}

impl AsMut<OggPage> for OggPageBuf {
    fn as_mut(&mut self) -> &mut OggPage {
        OggPage::from_u8_slice_unchecked_mut(&mut self.inner)
    }
}

impl ops::Deref for OggPageBuf {
    type Target = OggPage;

    fn deref<'a>(&'a self) -> &'a OggPage {
        OggPage::from_u8_slice_unchecked(&self.inner)
    }
}

impl Borrow<OggPage> for OggPageBuf {
    fn borrow(&self) -> &OggPage {
        OggPage::from_u8_slice_unchecked(&self.inner)
    }
}

impl BorrowMut<OggPage> for OggPageBuf {
    fn borrow_mut(&mut self) -> &mut OggPage {
        OggPage::from_u8_slice_unchecked_mut(&mut self.inner)
    }
}

impl ToOwned for OggPage {
    type Owned = OggPageBuf;

    fn to_owned(&self) -> OggPageBuf {
        OggPageBuf { inner: self.inner.to_owned() }
    }
}

impl OggPageBuf {
    pub fn new(mut buf: Vec<u8>) -> Result<OggPageBuf, OggPageCheckError>  {
        let slice_len = try!(OggPage::measure_whole(&buf)).len();
        buf.truncate(slice_len);
        Ok(OggPageBuf { inner: buf })
    }

    pub fn empty() -> OggPageBuf {
        let empty_page = [
            b'O', b'g', b'g', b'S', // capture
            0, // version
            0, // header type
            0, 0, 0, 0, 0, 0, 0, 0, // granule position
            0, 0, 0, 0, // bitstream serial number
            0, 0, 0, 0, // page seq number
            0x11, 0xA5, 0xA1, 0x9E, // checksum
            0, // page segments lengths
        ].to_vec();

        debug_assert!(OggPage::new(&empty_page[..]).is_ok());
        OggPageBuf { inner: empty_page.to_vec() }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.inner
    }

    pub fn into_cow(self) -> Cow<'static, OggPage> {
        Cow::Owned(self)
    }
}

impl OggPage {
    /// The following function allows unchecked construction of a ogg page
    /// from a u8 slice.  This is private because it does not maintain
    /// the OggPage invariant.
    fn from_u8_slice_unchecked(s: &[u8]) -> &OggPage {
        unsafe { mem::transmute(s) }
    }

    /// The following (private!) function allows unchecked construction of a
    /// mutable ogg page from a mutable u8 slice.  This is private because it
    /// does not maintain the OggPage invariant.
    fn from_u8_slice_unchecked_mut(s: &mut [u8]) -> &mut OggPage {
        unsafe { mem::transmute(s) }
    }

    pub fn as_u8_slice(&self) -> &[u8] {
        unsafe { mem::transmute(self) }
    }

    /// Mutably borrow the underlying storage.  This is private because it
    /// does not maintain the OggPage invariant.
    fn as_u8_slice_mut(&mut self) -> &mut [u8] {
        unsafe { mem::transmute(self) }
    }

    pub fn new(buf: &[u8]) -> Result<&OggPage, OggPageCheckError> {
        let buffer = try!(OggPage::measure_whole(buf));
        let page = OggPage::from_u8_slice_unchecked(buffer);
        try!(page.validate_checksum());
        Ok(page)
    }

    pub fn new_mut(buf: &mut [u8]) -> Result<&mut OggPage, OggPageCheckError> {
        let page_length = {
            let (hbuf, bbuf) = try!(OggPage::split_components(buf));
            hbuf.len() + bbuf.len()
        };
        let page = OggPage::from_u8_slice_unchecked_mut(&mut buf[0..page_length]);
        try!(page.validate_checksum());
        Ok(page)
    }

    fn split_components(buf: &[u8]) -> Result<(&[u8], &[u8]), OggPageCheckError> {
        impl From<ByteOrderError> for OggPageCheckError {
            fn from(e: ByteOrderError) -> OggPageCheckError {
                match e {
                    ByteOrderError::UnexpectedEOF => OggPageCheckError::TooShort,
                    ByteOrderError::Io(_) => panic!("no I/O allowed"),
                }
            }
        }

        let mut cursor = Cursor::new(buf);
        if buf.len() < 27 {
            return Err(OggPageCheckError::TooShort);
        }
        if &buf[0..4] != OGG_PAGE_CAPTURE {
            return Err(OggPageCheckError::BadCapture);
        }

        cursor.consume(4);  // capture sequence
        if try!(cursor.read_u8()) != 0 {
            return Err(OggPageCheckError::BadVersion);
        }

        // flags(1) + granule(8) + serial(4) + page_seq(4) + csum(4)
        cursor.consume(1 + 8 + 4 + 4 + 4);

        let page_segments = try!(cursor.read_u8());

        let mut body_len = 0;
        for _ in 0..page_segments {
            body_len += try!(cursor.read_u8()) as u64;
        }

        let total_len = (cursor.position() + body_len) as usize;
        if buf.len() < total_len {
            return Err(OggPageCheckError::TooShort);
        }

        let h_end = cursor.position() as usize;
        let b_end = h_end + body_len as usize;
        Ok((
            &buf[0..h_end],
            &buf[h_end..b_end],
        ))
    }

    fn measure_whole(buf: &[u8]) -> Result<&[u8], OggPageCheckError> {
        let page_length = {
            let (h_buf, b_buf) = try!(OggPage::split_components(buf));
            h_buf.len() + b_buf.len()
        };
        Ok(&buf[0..page_length])
    }

    pub fn position(&self) -> u64 {
        let self_buf = self.as_u8_slice();
        let mut cur = Cursor::new(&self_buf[POSITION_OFFSET..POSITION_OFFSET+8]);
        cur.read_u64::<LittleEndian>().unwrap()
    }

    pub fn set_position(&mut self, granule: u64) {
        let mut tx = self.begin();
        tx.set_position(granule);
    }

    pub fn serial(&self) -> u32 {
        let self_buf = self.as_u8_slice();
        let mut cur = Cursor::new(&self_buf[SERIAL_OFFSET..SERIAL_OFFSET+4]);
        cur.read_u32::<LittleEndian>().unwrap()
    }

    pub fn set_serial(&mut self, serial: u32) {
        let mut tx = self.begin();
        tx.set_serial(serial);
    }

    pub fn continued(&self) -> bool {
        const FLAG: u8 = 0x01;

        let self_buf = self.as_u8_slice();
        (self_buf[5] & FLAG) > 0
    }

    pub fn set_continued(&mut self, value: bool) {
        let mut tx = self.begin();
        tx.set_continued(value);
    }

    pub fn bos(&self) -> bool {
        const FLAG: u8 = 0x02;

        let self_buf = self.as_u8_slice();
        (self_buf[5] & FLAG) > 0
    }

    pub fn set_bos(&mut self, value: bool) {
        let mut tx = self.begin();
        tx.set_bos(value);
    }

    pub fn eos(&self) -> bool {
        const FLAG: u8 = 0x04;

        let self_buf = self.as_u8_slice();
        (self_buf[5] & FLAG) > 0
    }

    pub fn set_eos(&mut self, value: bool) {
        let mut tx = self.begin();
        tx.set_eos(value);
    }


    pub fn sequence(&self) -> u32 {
        let self_buf = self.as_u8_slice();
        let mut cur = Cursor::new(&self_buf[SEQUENCE_OFFSET..SEQUENCE_OFFSET+4]);
        cur.read_u32::<LittleEndian>().unwrap()
    }

    pub fn set_sequence(&mut self, serial: u32) {
        let mut tx = self.begin();
        tx.set_sequence(serial);
    }

    fn checksum_helper(&self) -> u32 {
        let self_buf = self.as_u8_slice();

        let mut crc32 = 0;
        for (idx, &byte) in self_buf.iter().enumerate() {
            if CHECKSUM_OFFSET <= idx && idx < CHECKSUM_OFFSET + 4 {
                crc::crc32(&mut crc32, 0);
            } else {
                crc::crc32(&mut crc32, byte);
            }
        }
        crc32
    }

    fn validate_checksum(&self) -> Result<(), OggPageCheckError> {
        let computed = self.checksum_helper();

        let self_buf = self.as_u8_slice();
        let crc_buf = &self_buf[CHECKSUM_OFFSET..CHECKSUM_OFFSET+4];
        let in_page = LittleEndian::read_u32(crc_buf);

        if computed == in_page {
            Ok(())
        } else {
            Err(OggPageCheckError::BadCrc)
        }
    }

    fn recompute_checksum(&mut self) {
        let crc32 = self.checksum_helper();
        let self_buf = self.as_u8_slice_mut();
        let crc_buf = &mut self_buf[CHECKSUM_OFFSET..CHECKSUM_OFFSET+4];
        LittleEndian::write_u32(crc_buf, crc32);
    }

    pub fn begin<'a>(&'a mut self) -> ChecksumGuard<'a> {
        ChecksumGuard {
            page: self,
            _marker: PhantomData,
        }
    }

    pub fn header(&self) -> &[u8] {
        let slice: &[u8] = self.as_u8_slice();
        let (header, _body) = OggPage::split_components(slice).unwrap();
        header
    }

    pub fn body(&self) -> &[u8] {
        let slice: &[u8] = self.as_u8_slice();
        let (_header, body) = OggPage::split_components(slice).unwrap();
        body
    }

    /// Am iterator of packet slices
    pub fn raw_packets<'a>(&'a self) -> RawPackets<'a> {
        let slice: &[u8] = self.as_u8_slice();
        let packet_count = slice[26] as usize;
        RawPackets {
            page: &self,
            packet: 0,
            packet_offset: 27,
            packet_count: packet_count,
            body_offset: 27 + packet_count,
        }
    }

    pub fn into_cow<'a>(&'a self) -> Cow<'a, OggPage> {
        Cow::Borrowed(self)
    }
}

pub struct RawPackets<'a> {
    page: &'a OggPage,

    // the current packet number
    packet: usize,

    // the total number of packets
    packet_count: usize,

    // where the next packet size lies in the page
    packet_offset: usize,

    // where the next packet lies in the page
    body_offset: usize,
}

impl<'a> Iterator for RawPackets<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.packet_count <= self.packet {
            return None;
        }

        let slice = self.page.as_u8_slice();

        let offset = self.body_offset;
        let mut length: usize = 0;

        while self.packet < self.packet_count {
            let adding = slice[self.packet_offset + self.packet] as usize;
            length += adding;
            self.packet += 1;
            if adding < 255 {
                break;
            }
        }
        self.body_offset += length;
        let pkt = &slice[offset..][..length];
        Some(pkt)
    }
}

pub struct ChecksumGuard<'a> {
    page: &'a mut OggPage,
    _marker: PhantomData<&'a ()>,
}

impl<'a> ChecksumGuard<'a> {
    pub fn set_position(&mut self, granule: u64) {
        let self_buf = self.page.as_u8_slice_mut();
        let pos_slice = &mut self_buf[POSITION_OFFSET..POSITION_OFFSET+8];
        LittleEndian::write_u64(pos_slice, granule);
    }

    pub fn set_serial(&mut self, serial: u32) {
        let self_buf = self.page.as_u8_slice_mut();
        let ser_slice = &mut self_buf[SERIAL_OFFSET..SERIAL_OFFSET+4];
        LittleEndian::write_u32(ser_slice, serial);
    }

    pub fn set_sequence(&mut self, sequence: u32) {
        let self_buf = self.page.as_u8_slice_mut();
        let seq_slice = &mut self_buf[SEQUENCE_OFFSET..SEQUENCE_OFFSET+4];
        LittleEndian::write_u32(seq_slice, sequence);
    }

    pub fn set_continued(&mut self, value: bool) {
        const FLAG: u8 = 0x01;

        let self_buf = self.page.as_u8_slice_mut();
        let header_value = self_buf[5] & (0xFF ^ FLAG);
        let add_this = if value { FLAG } else { 0x00 };
        self_buf[5] = header_value | add_this;
    }

    pub fn set_bos(&mut self, value: bool) {
        const FLAG: u8 = 0x02;

        let self_buf = self.page.as_u8_slice_mut();
        let header_value = self_buf[5] & (0xFF ^ FLAG);
        let add_this = if value { FLAG } else { 0x00 };
        self_buf[5] = header_value | add_this;
    }

    pub fn set_eos(&mut self, value: bool) {
        const FLAG: u8 = 0x04;

        let self_buf = self.page.as_u8_slice_mut();
        let header_value = self_buf[5] & (0xFF ^ FLAG);
        let add_this = if value { FLAG } else { 0x00 };
        self_buf[5] = header_value | add_this;
    }

    pub fn scoped<F>(self, func: F) where F: Fn(ChecksumGuard<'a>) {
        func(self)
    }
}

impl<'a> Drop for ChecksumGuard<'a> {
    fn drop(&mut self) {
        self.page.recompute_checksum();
    }
}

pub struct OggBuilder {
    lengths: Vec<usize>,
    buffer: Vec<u8>,
}

impl OggBuilder {
    pub fn new() -> OggBuilder {
        OggBuilder {
            lengths: Vec::new(),
            buffer: Vec::new(),
        }
    }

    pub fn add_packet(&mut self, packet: &[u8]) {
        self.lengths.push(packet.len());
        self.buffer.extend(packet);
    }

    pub fn build(&self) -> Result<OggPageBuf, ()> {
        let mut segment_count = 0;

        // compute size
        for &length in self.lengths.iter() {
            segment_count += length / 255;
            segment_count += 1;
        }

        if 255 < segment_count {
            return Err(());
        }

        let mut header = OggPageBuf::empty().into_inner();
        header.pop().unwrap(); // pop the ending zero
        header.push(segment_count as u8);

        for &length in self.lengths.iter() {
            let mut length: usize = length;
            while 255 <= length {
                length -= 255;
                header.push(255);
            }
            header.push(length as u8);
        }

        header.extend(&self.buffer[..]);
        let prelen = header.len();
        let page = OggPageBuf::new(header).unwrap();
        assert_eq!(prelen, page.as_u8_slice().len());
        Ok(page)
    }
}

pub struct Recapture([u8; 4]);

impl Recapture {
    pub fn new() -> Recapture {
        Recapture([0; 4])
    }

    pub fn push_byte(&mut self, byte: u8) {
        let mut buf = [0; 4];
        buf[0] = self.0[1];
        buf[1] = self.0[2];
        buf[2] = self.0[3];
        buf[3] = byte;
        *self = Recapture(buf);
    }

    pub fn is_captured(&self) -> bool {
        &self.0 == OGG_PAGE_CAPTURE
    }
}

#[cfg(test)]
mod tests {
    use super::{OggTrack, OggPageBuf, Recapture};

    static SAMPLE_OGG: &'static [u8] = include_bytes!("../testdata/Hydrate-Kenny_Beltrey.ogg");

    #[test]
    fn test_capture() {
        let mut cap = Recapture::new();
        cap.push_byte(b'O');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'g');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'g');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'S');
        assert_eq!(true, cap.is_captured());
        cap.push_byte(b'g');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'g');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'S');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'O');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'g');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'g');
        assert_eq!(false, cap.is_captured());
        cap.push_byte(b'S');
        assert_eq!(true, cap.is_captured());
    }

    #[test]
    fn test_packets() {
        let track = OggTrack::new(SAMPLE_OGG).unwrap();
        let mut pages = track.pages();

        let page0 = pages.next().unwrap();
        let mut page0packets = page0.raw_packets();
        assert!(page0packets.next().unwrap().starts_with(b"\x01vorbis"));
        assert!(page0packets.next().is_none());


        let page1 = pages.next().unwrap();
        let mut page1packets = page1.raw_packets();
        assert!(page1packets.next().unwrap().starts_with(b"\x03vorbis"));
        assert!(page1packets.next().unwrap().starts_with(b"\x05vorbis"));
        assert!(page1packets.next().is_none());
    }

    #[test]
    fn test_ogg_page_buf() {
        let _ = OggPageBuf::empty();
    }
}
