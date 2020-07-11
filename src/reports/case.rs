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

use crate::reports::TestResult;
use chrono::Duration;
use std::borrow::Cow;
use std::io::Write;
use xml::writer::XmlEvent;
use xml::EventWriter;

/// Represents the execution of a single test case
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase<'a> {
    /// the name of the test
    name: Cow<'a, str>,
    /// the test group
    class: Cow<'a, str>,
    /// STDOUT of the test execution
    stdout: Cow<'a, str>,
    /// STDERR of the test execution
    stderr: Cow<'a, str>,
    /// The result of the test execution
    result: TestResult<'a>,
    /// The duration of the test execution
    time: Duration,
}

impl<'a> TestCase<'a> {
    pub fn new(
        name: &'_ str,
        class: &'_ str,
        result: &'_ TestResult<'a>,
        time: Duration,
    ) -> TestCase<'a> {
        TestCase {
            name: Cow::Owned(name.to_string()),
            class: Cow::Owned(class.to_string()),
            stdout: Default::default(),
            stderr: Default::default(),
            result: result.clone(),
            time,
        }
    }

    pub fn new_with_output(
        name: &'_ str,
        class: &'_ str,
        result: &'_ TestResult<'a>,
        stdout: Cow<'a, str>,
        stderr: Cow<'a, str>,
        time: Duration,
    ) -> TestCase<'a> {
        TestCase {
            name: Cow::Owned(name.to_string()),
            class: Cow::Owned(class.to_string()),
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            result: result.clone(),
            time,
        }
    }

    /// the name of the test case
    pub fn name(&'a self) -> &'a str {
        self.name.as_ref()
    }

    /// the test group name
    pub fn class(&'a self) -> &'a str {
        self.class.as_ref()
    }

    /// the test stdout
    pub fn stdout(&'a self) -> &'a str {
        self.stdout.as_ref()
    }

    /// the test stderr
    pub fn stderr(&'a self) -> &'a str {
        self.stderr.as_ref()
    }

    /// the test result
    pub fn result(&'a self) -> &'a TestResult<'a> {
        &self.result
    }

    /// the test duration
    pub fn time(&'a self) -> Duration {
        self.time
    }

    pub fn write<W: Write>(&self, writer: &mut EventWriter<W>) -> anyhow::Result<()> {
        let time = format!("{}", (self.time.num_milliseconds() as f64) / 1000.0);
        writer.write(
            XmlEvent::start_element("testcase")
                .attr("name", self.name.as_ref())
                .attr("classname", self.class.as_ref())
                .attr("time", &time),
        )?;
        match &self.result {
            TestResult::Success => (),
            TestResult::Failure { type_, message } => {
                writer.write(
                    XmlEvent::start_element("failure")
                        .attr("message", message.as_ref())
                        .attr("type", type_.as_ref()),
                )?;
                writer.write(XmlEvent::end_element())?;
            }
            TestResult::Error { type_, message } => {
                writer.write(
                    XmlEvent::start_element("error")
                        .attr("message", message.as_ref())
                        .attr("type", type_.as_ref()),
                )?;
                writer.write(XmlEvent::end_element())?;
            }
            TestResult::Skipped { message } => {
                writer
                    .write(XmlEvent::start_element("skipped").attr("message", message.as_ref()))?;
                writer.write(XmlEvent::end_element())?;
            }
        }
        if !self.stdout.is_empty() {
            writer.write(XmlEvent::start_element("system-out"))?;
            writer.write(XmlEvent::cdata(self.stdout.as_ref()))?;
            writer.write(XmlEvent::end_element())?;
        }
        if !self.stderr.is_empty() {
            writer.write(XmlEvent::start_element("system-err"))?;
            writer.write(XmlEvent::cdata(self.stderr.as_ref()))?;
            writer.write(XmlEvent::end_element())?;
        }
        writer.write(XmlEvent::end_element())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::reports::xml_util::round_trip_xml_output;
    use crate::reports::TestCase;
    use crate::reports::TestResult;
    use chrono::Duration;
    use std::borrow::Cow;
    use xml::EventWriter;

    #[test]
    fn round_trip_data() {
        let instance = TestCase::new_with_output(
            "foo",
            "bar",
            &TestResult::success(),
            Cow::Borrowed("standard output"),
            Cow::Borrowed("standard error"),
            Duration::milliseconds(123456789),
        );
        assert_eq!(instance.name(), "foo");
        assert_eq!(instance.class(), "bar");
        assert_eq!(instance.result(), &TestResult::success());
        assert_eq!(instance.stdout(), "standard output");
        assert_eq!(instance.stderr(), "standard error");
    }

    #[test]
    fn write_success_as_xml() {
        let mut out = Vec::<u8>::new();
        let mut sink = EventWriter::new_with_config(&mut out, round_trip_xml_output());
        TestCase::new(
            "foo",
            "bar",
            &TestResult::success(),
            Duration::milliseconds(123456789),
        )
        .write(&mut sink)
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out).as_ref(), "<?xml version=\"1.0\" encoding=\"utf-8\"?><testcase name=\"foo\" classname=\"bar\" time=\"123456.789\"/>");
    }

    #[test]
    fn write_output_as_xml() {
        let mut out = Vec::<u8>::new();
        let mut sink = EventWriter::new_with_config(&mut out, round_trip_xml_output());
        TestCase::new_with_output(
            "foo",
            "bar",
            &TestResult::success(),
            Cow::Borrowed("standard output"),
            Cow::Borrowed("standard error"),
            Duration::milliseconds(123456789),
        )
        .write(&mut sink)
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out).as_ref(), "<?xml version=\"1.0\" encoding=\"utf-8\"?><testcase name=\"foo\" classname=\"bar\" time=\"123456.789\"><system-out><![CDATA[standard output]]></system-out><system-err><![CDATA[standard error]]></system-err></testcase>");
    }

    #[test]
    fn write_skipped_as_xml() {
        let mut out = Vec::<u8>::new();
        let mut sink = EventWriter::new_with_config(&mut out, round_trip_xml_output());
        TestCase::new(
            "foo",
            "bar",
            &TestResult::skipped("reason"),
            Duration::milliseconds(123456789),
        )
        .write(&mut sink)
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out).as_ref(), "<?xml version=\"1.0\" encoding=\"utf-8\"?><testcase name=\"foo\" classname=\"bar\" time=\"123456.789\"><skipped message=\"reason\"/></testcase>");
    }

    #[test]
    fn write_failure_as_xml() {
        let mut out = Vec::<u8>::new();
        let mut sink = EventWriter::new_with_config(&mut out, round_trip_xml_output());
        TestCase::new(
            "foo",
            "bar",
            &TestResult::failure("reason"),
            Duration::milliseconds(123456789),
        )
        .write(&mut sink)
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out).as_ref(), "<?xml version=\"1.0\" encoding=\"utf-8\"?><testcase name=\"foo\" classname=\"bar\" time=\"123456.789\"><failure message=\"reason\" type=\"assertion\"/></testcase>");
    }

    #[test]
    fn write_error_as_xml() {
        let mut out = Vec::<u8>::new();
        let mut sink = EventWriter::new_with_config(&mut out, round_trip_xml_output());
        TestCase::new(
            "foo",
            "bar",
            &TestResult::error("reason"),
            Duration::milliseconds(123456789),
        )
        .write(&mut sink)
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&out).as_ref(), "<?xml version=\"1.0\" encoding=\"utf-8\"?><testcase name=\"foo\" classname=\"bar\" time=\"123456.789\"><error message=\"reason\" type=\"error\"/></testcase>");
    }
}
