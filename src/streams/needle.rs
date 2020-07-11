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

use std::str::FromStr;

use uuid::Uuid;

use crate::streams::{NEEDLE_END, NEEDLE_METADATA, NEEDLE_START};

/// An error that can occur while parsing a [`Needle`].
///
/// [`Needle`]: struct.Needle.html
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Error {
    /// Invalid Needle
    ///
    /// [`Needle`]: struct.Needle.html
    InvalidNeedle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Needle {
    id: String,
    metadata: Option<String>,
    filename: String,
}

impl Needle {
    /// Generates a needle for the specified filename.
    pub fn new(filename: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metadata: None,
            filename: filename.to_string(),
        }
    }
    /// Generates a needle for the specified filename with additional metadata.
    pub fn new_with_kind(filename: &str, kind: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metadata: Some(kind.to_string()),
            filename: filename.to_string(),
        }
    }

    pub fn from_bytes(s: &[u8]) -> Result<Self, Error> {
        if s.starts_with(NEEDLE_START) && s.ends_with(NEEDLE_END) {
            let s = &s[NEEDLE_START.len()..s.len() - NEEDLE_END.len()];
            if let Some(index) = s
                .windows(NEEDLE_METADATA.len())
                .position(|s| s == NEEDLE_METADATA)
            {
                let index2 = s
                    .windows(NEEDLE_METADATA.len())
                    .rposition(|s| s == NEEDLE_METADATA)
                    .unwrap_or(index);
                if index2 == index {
                    return Ok(Needle {
                        id: String::from_utf8_lossy(&s[..index]).to_string(),
                        metadata: None,
                        filename: String::from_utf8_lossy(&s[index + NEEDLE_METADATA.len()..])
                            .to_string(),
                    });
                } else {
                    return Ok(Needle {
                        id: String::from_utf8_lossy(&s[..index]).to_string(),
                        metadata: Some(
                            String::from_utf8_lossy(&s[index + NEEDLE_METADATA.len()..index2])
                                .to_string(),
                        ),
                        filename: String::from_utf8_lossy(&s[index2 + NEEDLE_METADATA.len()..])
                            .to_string(),
                    });
                }
            }
        }
        Err(Error::InvalidNeedle)
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn kind(&self) -> Option<&str> {
        match &self.metadata {
            None => None,
            Some(s) => Some(s.as_str()),
        }
    }

    pub fn find_start(buf: &[u8]) -> Option<usize> {
        buf.windows(NEEDLE_START.len())
            .position(|s| s == NEEDLE_START)
    }

    pub fn find(buf: &[u8]) -> Option<(usize, usize)> {
        if let Some(start) = Self::find_start(&buf) {
            if let Some(mid) = buf[start + NEEDLE_START.len()..]
                .windows(NEEDLE_METADATA.len())
                .position(|s| s == NEEDLE_METADATA)
            {
                let mid = start + NEEDLE_START.len() + mid;
                if let Some(end) = buf[mid + NEEDLE_METADATA.len()..]
                    .windows(NEEDLE_END.len())
                    .position(|s| s == NEEDLE_END)
                {
                    return Some((start, mid + NEEDLE_METADATA.len() + end + NEEDLE_END.len()));
                }
            }
        }
        None
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}

impl ToString for Needle {
    fn to_string(&self) -> String {
        match &self.metadata {
            None => format!(
                "{}{}{}{}{}",
                String::from_utf8_lossy(NEEDLE_START),
                self.id,
                String::from_utf8_lossy(NEEDLE_METADATA),
                self.filename,
                String::from_utf8_lossy(NEEDLE_END)
            ),
            Some(kind) => format!(
                "{}{}{}{}{}{}{}",
                String::from_utf8_lossy(NEEDLE_START),
                self.id,
                String::from_utf8_lossy(NEEDLE_METADATA),
                kind,
                String::from_utf8_lossy(NEEDLE_METADATA),
                self.filename,
                String::from_utf8_lossy(NEEDLE_END)
            ),
        }
    }
}

impl FromStr for Needle {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use crate::streams::needle::Error;
    use crate::streams::{Needle, NEEDLE_END, NEEDLE_START};

    #[test]
    fn round_trip_no_metadata() {
        let n = Needle::new("/foo/bar.txt");
        assert_eq!(n, n.to_string().parse().unwrap());
        assert_eq!(n.filename(), "/foo/bar.txt");
        assert_eq!(n.kind(), None);
    }

    #[test]
    fn round_trip_with_metadata() {
        let n = Needle::new_with_kind("/foo/bar.txt", "manchu");
        assert_eq!(n, n.to_string().parse().unwrap());
        assert_eq!(n.filename(), "/foo/bar.txt");
        assert_eq!(n.kind(), Some("manchu"));
    }

    #[test]
    fn parse_invalid1() {
        let n = Needle::new("/foo/bar.txt");
        let n = n.as_bytes();
        assert_eq!(Needle::from_bytes(&n[1..]), Err(Error::InvalidNeedle))
    }

    #[test]
    fn parse_invalid2() {
        let n = Needle::new("/foo/bar.txt");
        let n = n.as_bytes();
        assert_eq!(
            Needle::from_bytes(&n[..n.len() - 1]),
            Err(Error::InvalidNeedle)
        )
    }

    #[test]
    fn parse_invalid3() {
        let n = format!(
            "{}{}",
            String::from_utf8_lossy(NEEDLE_START),
            String::from_utf8_lossy(NEEDLE_END)
        )
        .into_bytes();
        assert_eq!(Needle::from_bytes(&n), Err(Error::InvalidNeedle))
    }

    #[test]
    fn find_valid_whole() {
        let n = Needle::new("/foo/bar.txt");
        let n = n.as_bytes();
        assert_eq!(Needle::find(&n), Some((0, n.len())))
    }

    #[test]
    fn find_valid_whole_with_kind() {
        let n = Needle::new_with_kind("/foo/bar.txt", "manchu");
        let n = n.as_bytes();
        assert_eq!(Needle::find(&n), Some((0, n.len())))
    }

    #[test]
    fn find_valid_within() {
        let n = Needle::new("/foo/bar.txt");
        let n = format!("prefix{}suffix", n.to_string()).into_bytes();
        assert_eq!(Needle::find(&n), Some((6, n.len() - 6)))
    }

    #[test]
    fn find_invalid1() {
        let n = Needle::new("/foo/bar.txt");
        let n = n.as_bytes();
        assert_eq!(Needle::find(&n[1..]), None)
    }

    #[test]
    fn find_invalid2() {
        let n = Needle::new("/foo/bar.txt");
        let n = n.as_bytes();
        assert_eq!(Needle::find(&n[..n.len() - 1]), None)
    }

    #[test]
    fn find_invalid3() {
        let n = format!(
            "{}{}",
            String::from_utf8_lossy(NEEDLE_START),
            String::from_utf8_lossy(NEEDLE_END)
        )
        .into_bytes();
        assert_eq!(Needle::find(&n), None)
    }
}
