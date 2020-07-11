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

use std::borrow::Cow;
use xml::attribute::Attribute;
use xml::writer::XmlEvent;
use xml::{EmitterConfig, ParserConfig};

/// XML writer configuration to give pretty output
pub fn pretty_xml_output() -> EmitterConfig {
    EmitterConfig {
        line_separator: Cow::Borrowed("\n"),
        indent_string: Cow::Borrowed("  "),
        perform_indent: true,
        perform_escaping: true,
        write_document_declaration: true,
        normalize_empty_elements: true,
        cdata_to_characters: false,
        keep_element_names_stack: true,
        autopad_comments: false,
        pad_self_closing: false,
    }
}

/// XML writer configuration to give round-tripped output when paired with [`round_trip_xml_input`]
pub fn round_trip_xml_output() -> EmitterConfig {
    EmitterConfig {
        line_separator: Default::default(),
        indent_string: Default::default(),
        perform_indent: false,
        perform_escaping: true,
        write_document_declaration: true,
        normalize_empty_elements: true,
        cdata_to_characters: false,
        keep_element_names_stack: true,
        autopad_comments: false,
        pad_self_closing: false,
    }
}

/// XML reader configuration to give round-tripped output when paired with [`round_trip_xml_output`]
pub(crate) fn round_trip_xml_input() -> ParserConfig {
    ParserConfig {
        trim_whitespace: false,
        whitespace_to_characters: false,
        cdata_to_characters: false,
        ignore_comments: false,
        coalesce_characters: true,
        extra_entities: Default::default(),
        ignore_end_of_stream: false,
        replace_unknown_entity_references: false,
        ignore_root_level_whitespace: false,
    }
}

pub trait ToWrite {
    fn to_write<'a>(&'a self) -> Vec<XmlEvent<'a>>;
}

impl ToWrite for xml::reader::XmlEvent {
    fn to_write(&self) -> Vec<XmlEvent> {
        match self {
            xml::reader::XmlEvent::StartDocument {
                version,
                encoding,
                standalone,
            } => vec![xml::writer::XmlEvent::StartDocument {
                version: *version,
                encoding: Some(encoding.as_str()),
                standalone: *standalone,
            }],
            xml::reader::XmlEvent::EndDocument => vec![],
            xml::reader::XmlEvent::ProcessingInstruction { name, data } => {
                vec![xml::writer::XmlEvent::processing_instruction(
                    name.as_str(),
                    match data {
                        Some(s) => Some(s.as_str()),
                        None => None,
                    },
                )]
            }
            xml::reader::XmlEvent::StartElement {
                name,
                attributes,
                namespace,
            } => {
                let attrs: Vec<Attribute> = attributes.iter().map(|a| a.borrow()).collect();
                vec![xml::writer::XmlEvent::StartElement {
                    name: name.borrow(),
                    attributes: Cow::Owned(attrs),
                    namespace: Cow::Borrowed(namespace),
                }]
            }
            xml::reader::XmlEvent::EndElement { name } => vec![xml::writer::XmlEvent::EndElement {
                name: Some(name.borrow()),
            }],
            xml::reader::XmlEvent::CData(data) => vec![xml::writer::XmlEvent::cdata(data.as_str())],
            xml::reader::XmlEvent::Comment(comment) => {
                vec![xml::writer::XmlEvent::comment(comment.as_str())]
            }
            xml::reader::XmlEvent::Characters(data) | xml::reader::XmlEvent::Whitespace(data) => {
                vec![xml::writer::XmlEvent::characters(data)]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::reports::xml_util::{round_trip_xml_input, round_trip_xml_output};
    use crate::reports::ToWrite;
    use std::io::Cursor;
    use xml::reader::XmlEvent;
    use xml::{EventReader, EventWriter};

    #[test]
    fn round_trip() {
        let xml = include_str!("../../test/xml/sample.xml");
        let buf = Cursor::new(xml.as_bytes());
        let source = EventReader::new_with_config(buf, round_trip_xml_input());
        let mut out = Vec::new();
        let mut sink = EventWriter::new_with_config(&mut out, round_trip_xml_output());
        for event in source {
            for e in event.unwrap().to_write() {
                sink.write(e).unwrap();
            }
        }
        let expected_buf = Cursor::new(xml.as_bytes());
        let mut expected = EventReader::new_with_config(expected_buf, round_trip_xml_input());
        let actual_buf = Cursor::new(&out);
        let mut actual = EventReader::new_with_config(actual_buf, round_trip_xml_input());
        loop {
            match (expected.next(), actual.next()) {
                (Err(_), Err(_)) => break,
                (Ok(expected), Ok(actual)) => {
                    assert_eq!(expected, actual);
                    if let XmlEvent::EndDocument = actual {
                        break;
                    }
                }
                _ => panic!("Not the same length"),
            }
        }
    }
}
