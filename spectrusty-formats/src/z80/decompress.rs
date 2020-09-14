/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
use std::io::{Read, Result};
use memchr::memchr;

pub(super) struct MemDecompress<'a> {
    fill: u8,
    repeat: usize,
    index: usize,
    cursor: usize,
    data: &'a [u8]
}

enum Decompressed<'a> {
    Data(&'a [u8]),
    Repeat(usize)
}

impl<'a> Read for MemDecompress<'a> {
    fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let buflen = buf.len();
        while let Some(deco) = self.decompress(buf.len()) {
            let fill_len = match deco {
                Decompressed::Data(slice) => {
                    let len = slice.len();
                    buf[..len].copy_from_slice(&slice);
                    len
                }
                Decompressed::Repeat(repeat) => {
                    for p in buf[..repeat].iter_mut() {
                        *p = self.fill;
                    }
                    repeat
                }
            };
            buf = &mut buf[fill_len..];
        }
        Ok(buflen - buf.len())
    }
}

impl<'a> MemDecompress<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        MemDecompress {
            fill: 0, repeat: 0, index: 0, cursor: 0, data
        }
    }

    fn decompress<'b>(&'b mut self, maxlen: usize) -> Option<Decompressed<'b>> where 'a: 'b {
        if maxlen == 0 {
            return None;
        }

        let repeat = self.repeat.min(maxlen);
        if repeat != 0 {
            self.repeat -= repeat;
            return Some(Decompressed::Repeat(repeat))
        }

        let data = &self.data[self.cursor..];
        'main: loop {
            let mut index = self.index;
            // copy
            if index != 0 {
                if index > maxlen {
                    index = maxlen;
                }
                self.index -= index;
                self.cursor += index;
                return Some(Decompressed::Data(&data[..index]))
            }

            match data.get(..data.len().min(4)) {
                // end of file
                Some(&[]) => return None,
                // repeat
                Some(&[0xED, 0xED, repeat, fill]) => {
                    self.cursor += 4;
                    self.fill = fill;
                    let mut repeat = repeat as usize;
                    if repeat > maxlen {
                        self.repeat = repeat - maxlen;
                        repeat = maxlen;
                    }
                    return Some(Decompressed::Repeat(repeat))
                }
                // discard
                Some(&[0xED, 0xED])|Some(&[0xED, 0xED, _]) => {
                    self.cursor = self.data.len();
                    return None;
                }
                _ => {}
            };
            // search
            loop {
                if let Some(found) = memchr(0xED, &data[index..]) {
                    if let Some(0xED) = data.get(index + found + 1) {
                        self.index = index + found;
                        continue 'main
                    }
                    else {
                        index += found + 1;
                    }
                }
                else {
                    self.index = data.len();
                    continue 'main
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_decompress(mut decompress: MemDecompress, expect: &[u8]) {
        let mut buffer = Vec::new();
        assert_eq!(decompress.read(&mut []).unwrap(), 0);
        decompress.cursor = 0;
        assert_eq!(expect.len(), decompress.read_to_end(&mut buffer).unwrap());
        assert_eq!(expect, &buffer[..]);
        for n in 1..expect.len() {
            decompress.cursor = 0;
            buffer.resize(n, 0);
            let mut expected = expect;
            while expected.len() != 0 {
                let len = decompress.read(&mut buffer[..]).unwrap();
                assert_eq!(len, buffer.len().min(expected.len()));
                assert_eq!(buffer[..len], expected[..len]);
                expected = &expected[len..];
            }
            assert_eq!(decompress.read(&mut buffer[..]).unwrap(), 0);
        }
    }

    #[test]
    fn decompress_works() {
        let decompress = MemDecompress::new(&[0xED,0xED,7,42,96,0xED,0xED,2,0xED]);
        test_decompress(decompress, &[42,42,42,42,42,42,42,96,0xED,0xED]);

        let decompress = MemDecompress::new(&[69,0xED,0xED,0,0xED,0xED,0,0xED,0xED,4,0xED]);
        test_decompress(decompress, &[69,0xED,0,0xED,0xED,0xED,0xED]);

        let decompress = MemDecompress::new(&[0,1,2,3,4,5,0xED]);
        test_decompress(decompress, &[0,1,2,3,4,5,0xED]);

        let decompress = MemDecompress::new(&[0xED, 0,1,2,3,4,5,0xED, 0xED]);
        test_decompress(decompress, &[0xED, 0,1,2,3,4,5]);

        let decompress = MemDecompress::new(&[33, 0xED, 0xED, 0xED]);
        test_decompress(decompress, &[33]);

        let decompress = MemDecompress::new(&[0xED, 0xED, 0xED]);
        test_decompress(decompress, &[]);

        let decompress = MemDecompress::new(&[0, 0xED, 0xED, 0]);
        test_decompress(decompress, &[0]);

        let decompress = MemDecompress::new(&[0xED, 0xED, 2, 0xED, 0xED, 1, 0xED, 2, 0xED]);
        test_decompress(decompress, &[0xED, 0xED, 0xED, 1, 0xED, 2, 0xED]);
    }
}
