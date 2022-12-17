/*
    Copyright (C) 2020-2022  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use core::iter::once;
use std::io::{copy, repeat, Read, Write, Result};

pub fn compress_write_all<W: Write>(data: &[u8], mut wr: W) -> Result<()> {
    let mut data_iter = data.iter().copied().enumerate();
    let mut index = 0;
    let mut count: u8 = 1;
    let mut prev = match data_iter.next() {
        Some((_, ch)) => ch,
        None => return Ok(())
    };

    let len = data.len();
    let terminator = (len, data[len - 1].wrapping_add(1));

    for (pos, ch) in data_iter.chain(once(terminator)) {
        if prev == ch && count < u8::max_value() {
            count += 1;
        }
        else {
            if count > 4 || (prev == 0xED && count > 1) {
                let rep_start = pos - count as usize;
                if rep_start > index {
                    wr.write_all(&data[index..rep_start])?;
                }
                wr.write_all(&[0xED, 0xED, count, prev])?;
                index = pos;
            }
            prev = ch;
            count = 1;
        }
    }

    if index < len {
        wr.write_all(&data[index..])?;
    }
    Ok(())
}

pub fn compress_repeat_write_all<W: Write>(byte: u8, mut length: usize, mut wr: W) -> Result<()> {
    let min_len = if byte == 0xED { 2 } else { 5 };
    while length >= min_len {
        let count = length.min(u8::max_value() as usize);
        wr.write_all(&[0xED, 0xED, count as u8, byte])?;
        length -= count;
    }
    if length > 0 {
        copy(&mut repeat(byte).take(length as u64), &mut wr)?;
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_repeat_works() {
        let mut buf = Vec::new();
        compress_repeat_write_all(42, 0, &mut buf).unwrap();
        assert!(buf.is_empty());
        buf.clear();
        compress_repeat_write_all(42, 1, &mut buf).unwrap();
        assert_eq!(&[42], &buf[..]);
        buf.clear();
        compress_repeat_write_all(0xED, 1, &mut buf).unwrap();
        assert_eq!(&[0xED], &buf[..]);
        buf.clear();
        compress_repeat_write_all(0xED, 2, &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,2,0xED], &buf[..]);
        buf.clear();
        compress_repeat_write_all(42, 4, &mut buf).unwrap();
        assert_eq!(&[42,42,42,42], &buf[..]);
        buf.clear();
        compress_repeat_write_all(42, 5, &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,5,42], &buf[..]);
        buf.clear();
        compress_repeat_write_all(42, 255, &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,255,42], &buf[..]);
        buf.clear();
        compress_repeat_write_all(42, 256, &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,255,42,42], &buf[..]);
        buf.clear();
        compress_repeat_write_all(42, 1000, &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,255,42,0xED,0xED,255,42,0xED,0xED,255,42,0xED,0xED,235,42], &buf[..]);
    }

    #[test]
    fn compress_works() {
        let mut buf = Vec::new();
        compress_write_all(&[], &mut buf).unwrap();
        assert!(buf.is_empty());
        buf.clear();
        compress_write_all(&[42], &mut buf).unwrap();
        assert_eq!(&[42], &buf[..]);
        buf.clear();
        compress_write_all(&[0xED], &mut buf).unwrap();
        assert_eq!(&[0xED], &buf[..]);
        buf.clear();
        compress_write_all(&[1,2,3,42,77], &mut buf).unwrap();
        assert_eq!(&[1,2,3,42,77], &buf[..]);
        buf.clear();
        compress_write_all(&[0xED,0xED], &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,2,0xED], &buf[..]);
        buf.clear();
        compress_write_all(&[69,0xED,0xED], &mut buf).unwrap();
        assert_eq!(&[69,0xED,0xED,2,0xED], &buf[..]);
        buf.clear();
        compress_write_all(&[0xED,69,0xED], &mut buf).unwrap();
        assert_eq!(&[0xED,69,0xED], &buf[..]);
        buf.clear();
        compress_write_all(&[0xED,69,0xED,42], &mut buf).unwrap();
        assert_eq!(&[0xED,69,0xED,42], &buf[..]);
        buf.clear();
        compress_write_all(&[0;255], &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,255,0], &buf[..]);
        buf.clear();
        compress_write_all(&[69;1000], &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,255,69,0xED,0xED,255,69,0xED,0xED,255,69,0xED,0xED,235,69], &buf[..]);
        buf.clear();
        compress_write_all(&[1,1,1,1,1,2,2,2,2], &mut buf).unwrap();
        assert_eq!(&[0xED,0xED,5,1,2,2,2,2], &buf[..]);
        buf.clear();
        compress_write_all(&[1,2,2,3,3,3,4,4,4,5,5,5,5,5,6,6,6,6,6,6], &mut buf).unwrap();
        assert_eq!(&[1,2,2,3,3,3,4,4,4,0xED,0xED,5,5,0xED,0xED,6,6], &buf[..]);
    }
}
