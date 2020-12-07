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

/// marker prefix for an embedded stream
const NEEDLE_START: &[u8] = b"\n[[juxr::stream::";
/// metadata separator within an embedded stream marker
const NEEDLE_METADATA: &[u8] = b"::";
/// marker suffic for an embedded stream
const NEEDLE_END: &[u8] = b"]]\n";
/// the maximum valid length of an embedded stream marker
const NEEDLE_MAX_LEN: usize = 8192;

mod import;
mod needle;
mod trim;

pub use import::EmbeddedStream;
pub use import::EmbeddedStreams;
pub use needle::Needle;
pub use trim::TrimFilterReader;
