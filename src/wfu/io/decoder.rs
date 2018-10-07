extern crate byteorder;

use self::byteorder::{ReadBytesExt, LE};
use std::io::Read;
use std::iter::repeat_with;
use std::marker::PhantomData;

pub struct CString(pub String);

pub struct Bytes<P>(pub Vec<u8>, PhantomData<P>);

pub struct PrefixedString<P>(pub String, PhantomData<P>);

pub struct PrefixedVec<P, E>(pub Vec<E>, PhantomData<P>);

pub struct DecoderCursor<R> {
    pub reader: R,
}

impl<R> DecoderCursor<R> {
    pub fn new(reader: R) -> DecoderCursor<R> {
        DecoderCursor { reader }
    }

    pub fn decode<T: Decoder<R>>(&mut self) -> T {
        Decoder::decode(self)
    }

    pub fn decode_n<T: Decoder<R>>(&mut self, size: usize) -> Vec<T> {
        repeat_with(|| self.decode()).take(size).collect::<Vec<_>>()
    }
}

pub trait Decoder<R> {
    fn decode(cur: &mut DecoderCursor<R>) -> Self;
}

impl<R: Read> Decoder<R> for i8 {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        cur.reader.read_i8().unwrap()
    }
}

impl<R: Read> Decoder<R> for u8 {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        cur.reader.read_u8().unwrap()
    }
}

impl<R: Read> Decoder<R> for i16 {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        cur.reader.read_i16::<LE>().unwrap()
    }
}

impl<R: Read> Decoder<R> for u16 {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        cur.reader.read_u16::<LE>().unwrap()
    }
}

impl<R: Read> Decoder<R> for i32 {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        cur.reader.read_i32::<LE>().unwrap()
    }
}

impl<R: Read> Decoder<R> for bool {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        cur.reader.read_u8().unwrap() != 0
    }
}

impl<R: Read, P: Decoder<R> + Into<usize>> Decoder<R> for PrefixedString<P> {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let bytes: Bytes<P> = cur.decode();
        PrefixedString(String::from_utf8(bytes.0).unwrap(), PhantomData)
    }
}

impl<R: Read> Decoder<R> for CString {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let value = repeat_with(|| cur.reader.read_u8().unwrap())
            .take_while(|c| *c != 0)
            .map(|c| c as char)
            .collect::<String>();
        CString(value)
    }
}

impl<R: Read, P: Decoder<R> + Into<usize>> Decoder<R> for Bytes<P> {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let prefix: usize = cur.decode::<P>().into();
        let mut buf: Vec<u8> = Vec::with_capacity(prefix.into());
        unsafe { buf.set_len(prefix) };
        cur.reader.read_exact(buf.as_mut()).unwrap();
        Bytes(buf, PhantomData)
    }
}

impl<R: Read, P: Decoder<R> + Into<usize>, E: Decoder<R>> Decoder<R> for PrefixedVec<P, E> {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let prefix: usize = cur.decode::<P>().into();
        PrefixedVec(cur.decode_n(prefix), PhantomData)
    }
}
