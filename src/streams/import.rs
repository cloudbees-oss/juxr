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

use crate::streams::{Needle, NEEDLE_END, NEEDLE_MAX_LEN, NEEDLE_METADATA, NEEDLE_START};
use std::io;
use std::io::{Read, Write};
use std::string::FromUtf8Error;

/// Represents a stream of `EmbeddedStream` instances.
pub struct EmbeddedStreams<'a, R, W> {
    inner: EmbeddedReader<R>,
    side_writer: &'a mut W,
    end_of_stream: bool,
}

/// Represents a single stream within an `EmbeddedStreams`
pub struct EmbeddedStream<'a, R> {
    inner: &'a mut EmbeddedReader<R>,
    metadata: Vec<u8>,
    needle: Vec<u8>,
    /// have we finished the stream
    end_of_stream: bool,
}

struct EmbeddedReader<R> {
    inner: R,
    buffer: Vec<u8>,
    /// how big the buffer is
    capacity: usize,
    /// how much have we read into the buffer
    available: usize,
    /// how much of the buffer has been read in and doesn't have the needle
    checked: usize,
    /// how much have we wrote out from the buffer
    position: usize,
}

impl<'a, R: Read, W: Write> EmbeddedStreams<'a, R, W> {
    /// Creates a new `EmbeddedStreams` from a `reader` where any non-stream output will be sent to
    /// the supplied `side_writer`.
    pub fn new(reader: R, side_writer: &'a mut W) -> EmbeddedStreams<'a, R, W> {
        EmbeddedStreams {
            inner: EmbeddedReader::with_capacity(NEEDLE_MAX_LEN, reader),
            side_writer,
            end_of_stream: false,
        }
    }

    /// Applies the supplied function to every stream in the embedded stream
    pub fn for_each<F>(mut self, f: F)
    where
        F: (Fn(&mut EmbeddedStream<'_, R>)),
    {
        loop {
            if self.end_of_stream {
                return;
            }
            if self.inner.position >= self.inner.checked {
                if self.inner.checked >= self.inner.available {
                    self.inner.position = 0;
                    self.inner.available = 0;
                } else {
                    // move the unchecked hunk (which must be less than the needle
                    let count = self.inner.available - self.inner.checked;
                    let tmp =
                        Vec::from(&self.inner.buffer[self.inner.checked..self.inner.available]);
                    self.inner.buffer[..count].copy_from_slice(&tmp);
                    self.inner.position = 0;
                    self.inner.available = count;
                }
                // the capacity is always at least 1 byte more than the needle length
                // thus we can alway read at least one byte
                assert!(self.inner.available < self.inner.capacity);
                let count = match self
                    .inner
                    .inner
                    .read(&mut self.inner.buffer[self.inner.available..self.inner.capacity])
                {
                    Ok(c) => c,
                    _ => {
                        self.end_of_stream = true;
                        return;
                    }
                };
                if count == 0 && self.inner.available == 0 {
                    // we read nothing and there is no remaining buffer
                    // this is the end of everything
                    self.end_of_stream = true;
                    return;
                }
                self.inner.available += count;
                match Needle::find_start(&self.inner.buffer[..self.inner.available]) {
                    Some(0) => {
                        // the needle is at the top of the buffer: start of stream
                        if let Some(mid) = self.inner.buffer
                            [NEEDLE_START.len()..self.inner.available]
                            .windows(NEEDLE_METADATA.len())
                            .position(|w| w == NEEDLE_METADATA)
                        {
                            let mid = NEEDLE_START.len() + mid; // add search offset
                                                                // we have the middle token, now look for the end token
                            if let Some(end) = self.inner.buffer
                                [mid + NEEDLE_METADATA.len()..self.inner.available]
                                .windows(NEEDLE_END.len())
                                .position(|w| w == NEEDLE_END)
                            {
                                let end = mid + NEEDLE_METADATA.len() + end; // add search offset
                                self.inner.position = end + NEEDLE_END.len(); // move after the end of the marker
                                self.inner.checked = self.inner.position;
                                // we have the all tokens
                                let stream_name =
                                    Vec::from(&self.inner.buffer[mid + NEEDLE_METADATA.len()..end]);
                                let stream_id =
                                    Vec::from(&self.inner.buffer[NEEDLE_START.len()..mid]);
                                let mut stream =
                                    EmbeddedStream::new(&stream_name, &stream_id, &mut self.inner)
                                        .unwrap();
                                f(&mut stream);
                                if !stream.end_of_stream {
                                    let mut dump = vec![0; 8192];
                                    loop {
                                        match stream.read(&mut dump) {
                                            Ok(0) => break,
                                            Err(_) => break,
                                            _ => (),
                                        }
                                    }
                                }
                                continue;
                            } else {
                                // we can skip this start
                                self.inner.checked = NEEDLE_START.len();
                            }
                        } else {
                            // we can skip this start
                            self.inner.checked = NEEDLE_START.len();
                        }
                    }
                    Some(index) => {
                        // the needle is in the buffer, only safe to pipe that far
                        self.inner.checked = index;
                    }
                    None => {
                        // the needle is not in the buffer
                        if self.inner.available < NEEDLE_MAX_LEN {
                            // these are the last remaining bytes before the end of inner
                            self.inner.checked = self.inner.available
                        } else {
                            // keep the trailing needle length minus 1 bytes until we
                            // have more as they could be a partial match of the start
                            // of the needle
                            self.inner.checked = self.inner.available - NEEDLE_MAX_LEN + 1
                        }
                    }
                }
            }
            if self.inner.checked > self.inner.position {
                if let Ok(count) = self
                    .side_writer
                    .write(&self.inner.buffer[self.inner.position..self.inner.checked])
                {
                    self.inner.position += count
                } else {
                    self.end_of_stream = true;
                    return;
                }
            }
        }
    }
}

impl<R> EmbeddedReader<R> {
    fn with_capacity(capacity: usize, inner: R) -> EmbeddedReader<R> {
        // the needle always starts with a newline which we will emit
        let mut buffer = Vec::<u8>::with_capacity(capacity);
        unsafe {
            buffer.set_len(capacity);
        }
        EmbeddedReader {
            inner,
            buffer,
            capacity,
            position: 0,
            available: 0,
            checked: 0,
        }
    }
}

impl<'a, R> EmbeddedStream<'a, R> {
    fn new(
        metadata: &[u8],
        id: &[u8],
        inner: &'a mut EmbeddedReader<R>,
    ) -> Result<EmbeddedStream<'a, R>, FromUtf8Error> {
        // the needle always starts with a newline which we will emit
        let needle = format!(
            "\n[[juxr::stream::{}::{}]]\n",
            String::from_utf8(Vec::from(id))?,
            String::from_utf8(Vec::from(metadata))?
        )
        .as_bytes()
        .to_vec();
        Ok(EmbeddedStream {
            inner,
            metadata: Vec::from(metadata),
            needle,
            end_of_stream: false,
        })
    }

    /// Returns the name of this stream
    pub fn name(&self) -> String {
        let offset = self
            .metadata
            .windows(NEEDLE_METADATA.len())
            .position(|w| w == NEEDLE_METADATA)
            .map(|p| p + NEEDLE_METADATA.len())
            .unwrap_or(0);
        String::from_utf8_lossy(&self.metadata[offset..]).into()
    }

    /// Returns the kind of file, if present
    pub fn kind(&self) -> Option<String> {
        self.metadata
            .windows(NEEDLE_METADATA.len())
            .position(|w| w == NEEDLE_METADATA)
            .map(|p| String::from_utf8_lossy(&self.metadata[..p]).into())
    }
}

impl<R: Read> Read for EmbeddedStream<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.end_of_stream {
            Ok(0)
        } else {
            if self.inner.position >= self.inner.checked {
                if self.inner.checked >= self.inner.available {
                    self.inner.position = 0;
                    self.inner.available = 0;
                } else {
                    // move the unchecked hunk (which must be less than the needle
                    let count = self.inner.available - self.inner.checked;
                    let tmp =
                        Vec::from(&self.inner.buffer[self.inner.checked..self.inner.available]);
                    self.inner.buffer[..count].copy_from_slice(&tmp);
                    self.inner.position = 0;
                    self.inner.available = count;
                }
                // the capacity is always at least 1 byte more than the needle length
                // thus we can alway read at least one byte
                assert!(self.inner.available < self.inner.capacity);
                let count = self
                    .inner
                    .inner
                    .read(&mut self.inner.buffer[self.inner.available..self.inner.capacity])?;
                if count == 0 && self.inner.available == 0 {
                    // we read nothing and there is no remaining buffer
                    // this is the end of everything
                    self.end_of_stream = true;
                    return Ok(0);
                }
                self.inner.available += count;
                match self.inner.buffer[..self.inner.available]
                    .windows(self.needle.len())
                    .position(|window| window == self.needle.as_slice())
                {
                    Some(0) => {
                        // the needle is at the top of the buffer: end of stream
                        self.end_of_stream = true;
                        self.inner.position = self.needle.len();
                        self.inner.checked = self.inner.position;
                        return Ok(0);
                    }
                    Some(index) => {
                        // the needle is in the buffer, only safe to read that far
                        self.inner.checked = index;
                    }
                    None => {
                        // the needle is not in the buffer
                        if self.inner.available < self.needle.len() {
                            // these are the last remaining bytes before the end of inner
                            self.inner.checked = self.inner.available
                        } else {
                            // keep the trailing needle length minus 1 bytes until we
                            // have more as they could be a partial match of the start
                            // of the needle
                            self.inner.checked = self.inner.available - self.needle.len() + 1
                        }
                    }
                }
            }
            let count = (&self.inner.buffer[self.inner.position..self.inner.checked]).read(buf)?;
            self.inner.position += count;
            Ok(count)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use crate::streams::{import::EmbeddedReader, EmbeddedStream, EmbeddedStreams};

    #[test]
    fn given_empty_input_then_returns_empty() {
        let input = concat!("").as_bytes();
        let mut state = EmbeddedReader::with_capacity(40, Cursor::new(input));
        let mut instance = EmbeddedStream::new(b"foo.txt", b"cafebabe", &mut state).unwrap();
        let mut buf = vec![0; 300 as usize];
        let mut total = 0;
        loop {
            let count = instance.read(&mut buf[total..]).unwrap();
            if count == 0 {
                break;
            }
            total += count;
        }
        assert_eq!(total, 0);
    }

    #[test]
    fn given_empty_content_then_returns_empty() {
        let input =
            concat!("\n", "[[juxr::stream::cafebabe::foo.txt]]\n", "More text\n").as_bytes();
        let mut state = EmbeddedReader::with_capacity(40, Cursor::new(input));
        let mut instance = EmbeddedStream::new(b"foo.txt", b"cafebabe", &mut state).unwrap();
        let mut buf = vec![0; 300 as usize];
        let mut total = 0;
        loop {
            let count = instance.read(&mut buf[total..]).unwrap();
            if count == 0 {
                break;
            }
            total += count;
        }
        assert_eq!(total, 0);
    }

    #[test]
    fn given_content_without_needle_then_returns_content() {
        let input = concat!("Some text\n", "More text\n").as_bytes();
        let mut state = EmbeddedReader::with_capacity(40, Cursor::new(input));
        let mut instance = EmbeddedStream::new(b"foo.txt", b"cafebabe", &mut state).unwrap();
        let mut buf = vec![0; 300 as usize];
        let mut total = 0;
        loop {
            let count = instance.read(&mut buf[total..]).unwrap();
            if count == 0 {
                break;
            }
            total += count;
        }
        let expected = concat!("Some text\n", "More text\n");
        assert_eq!(
            String::from_utf8(Vec::from(&buf[..total])).unwrap(),
            expected
        )
    }

    #[test]
    fn given_content_with_needle_then_returns_content_up_to_needle() {
        let input = concat!(
            "Some text\n",
            "More text\n",
            "[[juxr::stream::cafebabe::foo.txt]]\n",
            "Ignored text\n"
        )
        .as_bytes();
        let mut state = EmbeddedReader::with_capacity(40, Cursor::new(input));
        let mut instance = EmbeddedStream::new(b"foo.txt", b"cafebabe", &mut state).unwrap();
        let mut buf = vec![0; 300 as usize];
        let mut total = 0;
        loop {
            let count = instance.read(&mut buf[total..]).unwrap();
            if count == 0 {
                break;
            }
            total += count;
        }
        let expected = concat!("Some text\n", "More text");
        assert_eq!(
            String::from_utf8(Vec::from(&buf[..total])).unwrap(),
            expected
        )
    }

    #[test]
    fn given_no_streams_then_flushes_to_out() {
        let input = concat!("Some text\n", "More text\n",).as_bytes();
        let mut out = Vec::<u8>::new();
        EmbeddedStreams::new(Cursor::new(input), &mut out).for_each(|stream| {
            println!("{}", stream.name());
        });

        assert_eq!(out.as_slice(), input);
    }

    #[test]
    fn given_stream_then_non_stream_flushes_to_out() {
        let input = concat!(
            "Some text\n",
            "\n",
            "[[juxr::stream::cafebabe::file.txt]]\n",
            "Some content\n",
            "[[juxr::stream::cafebabe::file.txt]]\n",
            "More text\n",
        )
        .as_bytes();

        let mut out = Vec::new();

        EmbeddedStreams::new(Cursor::new(input), &mut out).for_each(|stream| {
            let mut buf = vec![0; 300 as usize];
            let mut total = 0;
            loop {
                let count = stream.read(&mut buf[total..]).unwrap();
                if count == 0 {
                    break;
                }
                total += count;
            }
            eprintln!("{}: {}", stream.name(), String::from_utf8(buf).unwrap());
        });

        let expected = concat!("Some text\n", "More text\n",).as_bytes();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            String::from_utf8(Vec::from(expected)).unwrap()
        );
    }
}
