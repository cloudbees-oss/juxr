/*
 * Copyright (c) 2020 Stephen Connolly and CloudBees, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *     http://www.apache.org/licenses/LICENSE-2.0
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::io::Read;
use std::{cmp, io};

/// A filtering reader that strips out whitespace, ASCII control characters and any non strict
/// US-ASCII bytes for use in cleaning a stream before Base64 decoding.
pub struct TrimFilterReader<R> {
    inner: R,
    buffer: Vec<u8>,
    available: usize,
    position: usize,
}

impl<R: Read> TrimFilterReader<R> {
    pub fn new(inner: R) -> TrimFilterReader<R> {
        Self::with_capacity(inner, crate::streams::NEEDLE_MAX_LEN)
    }

    pub fn with_capacity(inner: R, capacity: usize) -> TrimFilterReader<R> {
        let mut buf = Vec::<u8>::with_capacity(capacity);
        unsafe {
            buf.set_len(capacity);
        }
        TrimFilterReader {
            inner,
            buffer: buf,
            available: 0,
            position: 0,
        }
    }
}

impl<R: Read> Read for TrimFilterReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.available {
            match self.inner.read(&mut self.buffer) {
                Ok(count) => {
                    if count == 0 {
                        return Ok(0);
                    }
                    self.position = 0;
                    self.available = count;
                }
                Err(e) => return Err(e),
            }
        }
        let mut offset = 0;
        let cap = cmp::max(1, buf.len());
        while offset < cap && self.position < self.available {
            let count = cmp::min(
                self.buffer[self.position..self.available]
                    .iter()
                    .position(|c| *c <= 32 || *c >= 128)
                    .unwrap_or_else(|| self.available - self.position),
                cap - offset,
            );
            if count == 0 {
                self.position += 1;
            } else if count == 1 {
                buf[offset] = self.buffer[self.position];
                self.position += 1;
                offset += 1;
            } else {
                buf[offset..offset + count]
                    .copy_from_slice(&self.buffer[self.position..self.position + count]);
                self.position += count;
                offset += count;
            }
        }
        Ok(offset)
    }
}

#[cfg(test)]
mod tests {
    use crate::streams::TrimFilterReader;
    use std::io::Cursor;
    use std::io::Read;

    #[test]
    fn strips_spaces() {
        let input = "this is a string with spaces and newlines".as_bytes();
        let mut instance = TrimFilterReader::new(Cursor::new(input));
        let mut output = String::new();
        let count = instance.read_to_string(&mut output);
        assert_eq!(count.unwrap(), 34);
        assert_eq!(output, "thisisastringwithspacesandnewlines".to_string());
    }
}
