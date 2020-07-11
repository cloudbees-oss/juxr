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

use crate::reports::xml_util::{round_trip_xml_input, round_trip_xml_output};
use crate::reports::ToWrite;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::io::{Read, Write};
use xml::attribute::OwnedAttribute;
use xml::{EventReader, EventWriter};

/// Processes and optionally transforms a JUnit XML Report.
#[derive(Default, Clone, PartialEq, Debug)]
pub struct ReportProcessor {
    test_suite_name_prefix: String,
    test_suite_name_suffix: String,
    test_case_name_prefix: String,
    test_case_name_suffix: String,
    test_case_class_prefix: String,
    test_case_class_suffix: String,
    attachment_prefix: String,
    attachment_windows_paths: bool,
    attachments: Vec<String>,
    secrets: Vec<String>,
}

impl ReportProcessor {
    pub fn new() -> ReportProcessor {
        ReportProcessor {
            ..Default::default()
        }
    }

    pub fn reset(&self) -> ReportProcessor {
        ReportProcessor {
            attachments: Vec::new(),
            ..self.clone()
        }
    }

    pub fn test_suite_name_prefix(self, test_suite_name_prefix: &str) -> ReportProcessor {
        ReportProcessor {
            test_suite_name_prefix: test_suite_name_prefix.to_string(),
            ..self
        }
    }

    pub fn test_suite_name_suffix(self, test_suite_name_suffix: &str) -> ReportProcessor {
        ReportProcessor {
            test_suite_name_suffix: test_suite_name_suffix.to_string(),
            ..self
        }
    }

    pub fn test_case_name_prefix(self, test_case_name_prefix: &str) -> ReportProcessor {
        ReportProcessor {
            test_case_name_prefix: test_case_name_prefix.to_string(),
            ..self
        }
    }

    pub fn test_case_name_suffix(self, test_case_name_suffix: &str) -> ReportProcessor {
        ReportProcessor {
            test_case_name_suffix: test_case_name_suffix.to_string(),
            ..self
        }
    }

    pub fn test_case_class_prefix(self, test_case_class_prefix: &str) -> ReportProcessor {
        ReportProcessor {
            test_case_class_prefix: test_case_class_prefix.to_string(),
            ..self
        }
    }

    pub fn test_case_class_suffix(self, test_case_class_suffix: &str) -> ReportProcessor {
        ReportProcessor {
            test_case_class_suffix: test_case_class_suffix.to_string(),
            ..self
        }
    }

    pub fn attachment_prefix(self, attachment_prefix: &str) -> ReportProcessor {
        ReportProcessor {
            attachment_prefix: attachment_prefix.to_string(),
            ..self
        }
    }

    pub fn secret(self, secret: &str) -> ReportProcessor {
        ReportProcessor {
            secrets: {
                let mut secrets = self.secrets;
                secrets.push(secret.to_string());
                // modified sort so that longer secrets are redacted first
                secrets.sort_by(|a, b| {
                    if a == b {
                        Ordering::Equal
                    } else if a.contains(b) {
                        Ordering::Less
                    } else if b.contains(a) {
                        Ordering::Greater
                    } else {
                        a.cmp(b)
                    }
                });
                secrets.dedup();
                secrets
            },
            ..self
        }
    }

    pub fn secrets(self, secrets: &[&str]) -> ReportProcessor {
        ReportProcessor {
            secrets: {
                let mut secrets: Vec<String> = secrets.iter().map(|s| s.to_string()).collect();
                // modified sort so that longer secrets are redacted first
                secrets.sort_by(|a, b| {
                    if a == b {
                        Ordering::Equal
                    } else if a.contains(b) {
                        Ordering::Less
                    } else if b.contains(a) {
                        Ordering::Greater
                    } else {
                        a.cmp(b)
                    }
                });
                secrets.dedup();
                secrets
            },
            ..self
        }
    }

    pub fn attachments(&self) -> Vec<&str> {
        self.attachments.iter().map(|s| s.as_str()).collect()
    }

    pub fn process<R: Read, W: Write>(&mut self, reader: R, writer: &mut W) -> anyhow::Result<()> {
        let mut xpath_stack = Vec::new();
        let mut xpath = String::new();
        let source = EventReader::new_with_config(reader, round_trip_xml_input());
        // see https://github.com/jenkinsci/junit-attachments-plugin/blob/3db4f1724bddf0380ad24858d50fe551afb55e4c/src/main/java/hudson/plugins/junitattachments/GetTestDataMethodObject.java#L171-L206
        let attachment = Regex::new(r"(\s*)\[\[ATTACHMENT\|([^]]+)]](\s*)").unwrap();
        let mut sink = EventWriter::new_with_config(WriteAll::new(writer), round_trip_xml_output());
        for event in source {
            let event = event?;
            let event = match &event {
                xml::reader::XmlEvent::StartDocument { .. } => {
                    xpath.clear();
                    xpath_stack.clear();
                    event
                }
                xml::reader::XmlEvent::StartElement {
                    name,
                    attributes,
                    namespace,
                } => {
                    xpath_stack.push(xpath.clone());
                    xpath.push('/');
                    xpath.push_str(&name.local_name);
                    if &xpath == "/testsuite/testcase" {
                        let mut new_attrs = Vec::new();
                        for a in attributes.to_owned() {
                            if a.name.local_name.as_str() == "name" {
                                new_attrs.push(OwnedAttribute::new(
                                    a.name.clone(),
                                    format!(
                                        "{}{}{}",
                                        self.test_case_name_prefix,
                                        a.value,
                                        self.test_case_name_suffix
                                    ),
                                ))
                            } else if a.name.local_name.as_str() == "classname" {
                                new_attrs.push(OwnedAttribute::new(
                                    a.name.clone(),
                                    format!(
                                        "{}{}{}",
                                        self.test_case_class_prefix,
                                        a.value,
                                        self.test_case_class_suffix
                                    ),
                                ))
                            } else {
                                new_attrs.push(a)
                            }
                        }
                        xml::reader::XmlEvent::StartElement {
                            name: name.clone(),
                            namespace: namespace.clone(),
                            attributes: new_attrs,
                        }
                    } else if &xpath == "/testsuite" {
                        let mut new_attrs = Vec::new();
                        for a in attributes.to_owned() {
                            if a.name.local_name.as_str() == "name" {
                                new_attrs.push(OwnedAttribute::new(
                                    a.name.clone(),
                                    format!(
                                        "{}{}{}",
                                        self.test_suite_name_prefix,
                                        a.value,
                                        self.test_suite_name_suffix
                                    ),
                                ))
                            } else {
                                new_attrs.push(a)
                            }
                        }
                        xml::reader::XmlEvent::StartElement {
                            name: name.clone(),
                            namespace: namespace.clone(),
                            attributes: new_attrs,
                        }
                    } else if &xpath == "/testsuite/properties/property" {
                        let mut new_attrs = Vec::new();
                        for a in attributes.to_owned() {
                            if a.name.local_name.as_str() == "value" {
                                let mut value = Cow::Borrowed(&a.value);
                                for secret in &self.secrets {
                                    value = Cow::Owned(value.replace(secret, "****"));
                                }
                                new_attrs
                                    .push(OwnedAttribute::new(a.name.clone(), value.to_string()))
                            } else {
                                new_attrs.push(a)
                            }
                        }
                        xml::reader::XmlEvent::StartElement {
                            name: name.clone(),
                            namespace: namespace.clone(),
                            attributes: new_attrs,
                        }
                    } else {
                        event
                    }
                }
                xml::reader::XmlEvent::EndElement { .. } => {
                    xpath = xpath_stack.pop().unwrap_or_else(|| "".to_string());
                    event
                }
                xml::reader::XmlEvent::CData(text) => {
                    let mut text = attachment.replace_all(text, |caps: &Captures| {
                        let file_name = caps.get(2).unwrap().as_str().to_string();
                        self.attachments.push(file_name.replace('\\', "/"));
                        let file_name = if self.attachment_windows_paths {
                            file_name.replace('/', "\\")
                        } else {
                            file_name
                        };
                        format!(
                            "{}[[ATTACHMENT|{}{}]]{}",
                            caps.get(1).unwrap().as_str(),
                            self.attachment_prefix,
                            file_name,
                            caps.get(3).unwrap().as_str()
                        )
                    });
                    for secret in &self.secrets {
                        text = Cow::Owned(text.replace(secret, "****"));
                    }
                    xml::reader::XmlEvent::CData(text.to_string())
                }
                xml::reader::XmlEvent::Characters(text) => {
                    let mut text = attachment.replace_all(text, |caps: &Captures| {
                        let file_name = caps.get(2).unwrap().as_str().to_string();
                        self.attachments.push(file_name.replace('\\', "/"));
                        let file_name = if self.attachment_windows_paths {
                            file_name.replace('/', "\\")
                        } else {
                            file_name
                        };
                        format!(
                            "{}[[ATTACHMENT|{}{}]]{}",
                            caps.get(1).unwrap().as_str(),
                            self.attachment_prefix,
                            file_name,
                            caps.get(3).unwrap().as_str()
                        )
                    });
                    for secret in &self.secrets {
                        text = Cow::Owned(text.replace(secret, "****"));
                    }
                    xml::reader::XmlEvent::Characters(text.to_string())
                }
                _ => event,
            };
            for event in event.to_write() {
                sink.write(event)?;
            }
        }
        self.attachments.sort();
        self.attachments.dedup();
        Ok(())
    }
}

struct WriteAll<W: Write> {
    inner: W,
}

impl<W: Write> WriteAll<W> {
    fn new(inner: W) -> WriteAll<W> {
        WriteAll { inner }
    }
}

impl<W: Write> Write for WriteAll<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::reports::ReportProcessor;

    #[test]
    fn idempotent_empty() {
        let xml = include_str!("../../test/report/empty.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new();
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            xml.to_string().replace(" />", "/>").trim()
        );
    }

    #[test]
    fn rename_suite() {
        let xml = include_str!("../../test/report/empty.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new()
            .test_suite_name_prefix("aaa---")
            .test_suite_name_suffix("---bbb");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/empty-renamed.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn idempotent_one() {
        let xml = include_str!("../../test/report/one.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new();
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            xml.to_string().replace(" />", "/>").trim()
        );
    }

    #[test]
    fn rename_test() {
        let xml = include_str!("../../test/report/one.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new()
            .test_case_name_prefix("ccc---")
            .test_case_name_suffix("---ddd");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/one-test-renamed.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn rename_class() {
        let xml = include_str!("../../test/report/one.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new()
            .test_case_class_prefix("eee---")
            .test_case_class_suffix("---fff");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/one-class-renamed.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn idempotent_property() {
        let xml = include_str!("../../test/report/property.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new();
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            xml.to_string().replace(" />", "/>").trim()
        );
    }

    #[test]
    fn redact_property() {
        let xml = include_str!("../../test/report/property.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new().secret("property");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/property-redacted.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn idempotent_output() {
        let xml = include_str!("../../test/report/output.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new();
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            xml.to_string().replace(" />", "/>").trim()
        );
    }

    #[test]
    fn redact_output() {
        let xml = include_str!("../../test/report/output.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new().secret("text");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/output-redacted.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn redact_multiple_output1() {
        let xml = include_str!("../../test/report/output.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new()
            .secret("text")
            .secret("some text")
            .secret("text")
            .secret("an irrelevant secret");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/output-redact-multiple.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn redact_multiple_output2() {
        let xml = include_str!("../../test/report/output.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new().secret("some text").secret("text");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/output-redact-multiple.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn redact_multiple_output3() {
        let xml = include_str!("../../test/report/output.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new().secrets(&[
            "text",
            "some text",
            "text",
            "a long string that is not going to be replaced",
        ]);
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/output-redact-multiple.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn redact_multiple_output4() {
        let xml = include_str!("../../test/report/output.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new().secrets(&["some text", "text"]);
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/output-redact-multiple.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn idempotent_attachment() {
        let xml = include_str!("../../test/report/attachment.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new();
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            xml.to_string().replace(" />", "/>").trim()
        );
    }

    #[test]
    fn attachment_enumeration() {
        let xml = include_str!("../../test/report/attachment.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new();
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        let mut attachments = instance.attachments();
        attachments.sort();
        assert_eq!(
            attachments,
            vec!["/another/path", "/some/path", "/yet/another/path"]
        );
    }

    #[test]
    fn relocate_attachment() {
        let xml = include_str!("../../test/report/attachment.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor::new().attachment_prefix("/foo/bar");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/attachment-relocated.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }

    #[test]
    fn windows_path_attachment() {
        let xml = include_str!("../../test/report/attachment.xml");
        let buf = Cursor::new(xml.as_bytes());
        let mut instance = ReportProcessor {
            attachment_windows_paths: true,
            ..Default::default()
        }
        .attachment_prefix("C:\\foo");
        let mut out = Vec::new();
        let _ = instance.process(buf, &mut out);
        assert_eq!(
            String::from_utf8_lossy(&out).replace(" />", "/>").trim(),
            include_str!("../../test/report/attachment-windows.xml")
                .to_string()
                .replace(" />", "/>")
                .trim()
        );
    }
}
