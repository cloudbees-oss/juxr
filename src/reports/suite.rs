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

use crate::reports::{TestCase, TestResult};
use chrono::Duration;
use std::borrow::Cow;
use std::io::Write;
use std::ops::Add;
use xml::writer::XmlEvent;
use xml::EventWriter;

/// A collection of tests
#[derive(Debug, Clone)]
pub struct TestSuite<'a> {
    name: Cow<'a, str>,
    cases: Vec<TestCase<'a>>,
}

impl<'a> TestSuite<'a> {
    pub fn new(name: &'_ str) -> TestSuite<'a> {
        TestSuite {
            name: Cow::Owned(name.to_string()),
            cases: Vec::new(),
        }
    }

    pub fn push(self, case: TestCase<'a>) -> TestSuite<'a> {
        TestSuite {
            cases: {
                let mut cases = self.cases.clone();
                cases.push(case);
                cases
            },
            ..self
        }
    }

    fn totals(&self) -> (i32, i32, i32, i32, Duration) {
        let mut tests = 0;
        let mut failures = 0;
        let mut skipped = 0;
        let mut errors = 0;
        let mut time = Duration::milliseconds(0);
        for case in &self.cases {
            tests += 1;
            time = time.add(case.time());
            match &case.result() {
                TestResult::Success => (),
                TestResult::Failure { .. } => failures += 1,
                TestResult::Error { .. } => {
                    errors += 1;
                }
                TestResult::Skipped { .. } => {
                    skipped += 1;
                }
            }
        }
        (tests, failures, skipped, errors, time)
    }

    pub fn test_count(&self) -> i32 {
        self.totals().0
    }

    pub fn failure_count(&self) -> i32 {
        self.totals().1
    }

    pub fn skipped_count(&self) -> i32 {
        self.totals().2
    }

    pub fn error_count(&self) -> i32 {
        self.totals().3
    }

    pub fn time(&self) -> Duration {
        self.totals().4
    }

    pub fn write<W: Write>(&self, writer: &mut EventWriter<W>) -> anyhow::Result<()> {
        let (tests, failures, skipped, errors, time) = self.totals();
        let tests = format!("{}", tests);
        let failures = format!("{}", failures);
        let skipped = format!("{}", skipped);
        let errors = format!("{}", errors);
        let time = format!("{}", (time.num_milliseconds() as f64) / 1000.0);
        writer.write(
            XmlEvent::start_element("testsuite")
                .attr("xsi:noNamespaceSchemaLocation", "https://maven.apache.org/surefire/maven-surefire-plugin/xsd/surefire-test-report.xsd")
                .attr("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance")
                .attr("name", &self.name)
                .attr("tests", &tests)
                .attr("failures", &failures)
                .attr("skipped", &skipped)
                .attr("errors", &errors)
                .attr("time", &time)
        )?;
        for case in &self.cases {
            case.write(writer)?
        }
        writer.write(XmlEvent::end_element())?;
        Ok(())
    }

    pub fn as_exit_code(&self) -> i32 {
        for case in &self.cases {
            if let TestResult::Failure { .. } | TestResult::Error { .. } = &case.result() {
                return 1;
            }
        }
        0
    }

    pub fn as_start_str(&self) -> String {
        format!("Running {}", self.name)
    }

    pub fn as_end_str(&self) -> String {
        let (tests, failures, skipped, errors, time) = self.totals();
        let mut result = format!(
            "Tests run: {}, Failures: {}, Errors: {}, Skipped: {}, Time elapsed: {} sec {} - in {}",
            tests,
            failures,
            errors,
            skipped,
            (time.num_milliseconds() as f64) / 1000.0,
            if failures > 0 {
                "<<< FAILURE".to_string()
            } else if errors > 0 {
                "<<< ERROR".to_string()
            } else {
                "".to_string()
            },
            self.name
        );
        for case in &self.cases {
            match &case.result() {
                TestResult::Failure { type_, message } => result.push_str(&format!(
                    "\n{}({}) Time elapsed: {} <<< FAILURE!\n\t{}: {}",
                    case.name(),
                    case.class(),
                    (case.time().num_milliseconds() as f64) / 1000.0,
                    type_,
                    message
                )),
                TestResult::Error { type_, message } => result.push_str(&format!(
                    "\n{}({}) Time elapsed: {} <<< ERROR!\n\t{}: {}",
                    case.name(),
                    case.class(),
                    (case.time().num_milliseconds() as f64) / 1000.0,
                    type_,
                    message
                )),
                _ => (),
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::reports::{TestCase, TestResult, TestSuite};
    use chrono::Duration;

    #[test]
    fn start_str() {
        let s = TestSuite::new("foo");
        assert_eq!(s.as_start_str(), "Running foo".to_string())
    }

    #[test]
    fn empty() {
        let s = TestSuite::new("foo");
        assert_eq!(s.time(), Duration::milliseconds(0));
        assert_eq!(s.test_count(), 0);
        assert_eq!(s.failure_count(), 0);
        assert_eq!(s.skipped_count(), 0);
        assert_eq!(s.error_count(), 0);
        assert_eq!(s.as_exit_code(), 0);
        assert_eq!(
            s.as_end_str(),
            "Tests run: 0, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 0 sec  - in foo"
                .to_string()
        )
    }

    #[test]
    fn success() {
        let s = TestSuite::new("foo");
        let s = s.push(TestCase::new(
            "a",
            "foo",
            &TestResult::success(),
            Duration::milliseconds(1000),
        ));
        let s = s.push(TestCase::new(
            "b",
            "foo",
            &TestResult::success(),
            Duration::milliseconds(500),
        ));
        assert_eq!(s.time(), Duration::milliseconds(1500));
        assert_eq!(s.test_count(), 2);
        assert_eq!(s.failure_count(), 0);
        assert_eq!(s.skipped_count(), 0);
        assert_eq!(s.error_count(), 0);
        assert_eq!(s.as_exit_code(), 0);
        assert_eq!(
            s.as_end_str(),
            "Tests run: 2, Failures: 0, Errors: 0, Skipped: 0, Time elapsed: 1.5 sec  - in foo"
                .to_string()
        )
    }

    #[test]
    fn skipped() {
        let s = TestSuite::new("foo");
        let s = s.push(TestCase::new(
            "a",
            "foo",
            &TestResult::success(),
            Duration::milliseconds(1000),
        ));
        let s = s.push(TestCase::new(
            "b",
            "foo",
            &TestResult::skipped("because"),
            Duration::milliseconds(500),
        ));
        assert_eq!(s.time(), Duration::milliseconds(1500));
        assert_eq!(s.test_count(), 2);
        assert_eq!(s.failure_count(), 0);
        assert_eq!(s.skipped_count(), 1);
        assert_eq!(s.error_count(), 0);
        assert_eq!(s.as_exit_code(), 0);
        assert_eq!(
            s.as_end_str(),
            "Tests run: 2, Failures: 0, Errors: 0, Skipped: 1, Time elapsed: 1.5 sec  - in foo"
                .to_string()
        )
    }

    #[test]
    fn failed() {
        let s = TestSuite::new("foo");
        let s = s.push(TestCase::new(
            "a",
            "foo",
            &TestResult::success(),
            Duration::milliseconds(1000),
        ));
        let s = s.push(TestCase::new(
            "b",
            "foo",
            &TestResult::failure("because"),
            Duration::milliseconds(500),
        ));
        assert_eq!(s.time(), Duration::milliseconds(1500));
        assert_eq!(s.test_count(), 2);
        assert_eq!(s.failure_count(), 1);
        assert_eq!(s.skipped_count(), 0);
        assert_eq!(s.error_count(), 0);
        assert_eq!(s.as_exit_code(), 1);
        assert_eq!(
            s.as_end_str(),
            "Tests run: 2, Failures: 1, Errors: 0, Skipped: 0, Time elapsed: 1.5 sec <<< FAILURE - in foo\nb(foo) Time elapsed: 0.5 <<< FAILURE!\n\tassertion: because"
                .to_string()
        )
    }

    #[test]
    fn error() {
        let s = TestSuite::new("foo");
        let s = s.push(TestCase::new(
            "a",
            "foo",
            &TestResult::success(),
            Duration::milliseconds(1000),
        ));
        let s = s.push(TestCase::new(
            "b",
            "foo",
            &TestResult::error("because"),
            Duration::milliseconds(500),
        ));
        assert_eq!(s.time(), Duration::milliseconds(1500));
        assert_eq!(s.test_count(), 2);
        assert_eq!(s.failure_count(), 0);
        assert_eq!(s.skipped_count(), 0);
        assert_eq!(s.error_count(), 1);
        assert_eq!(s.as_exit_code(), 1);
        assert_eq!(
            s.as_end_str(),
            "Tests run: 2, Failures: 0, Errors: 1, Skipped: 0, Time elapsed: 1.5 sec <<< ERROR - in foo\nb(foo) Time elapsed: 0.5 <<< ERROR!\n\terror: because"
                .to_string()
        )
    }

    #[test]
    fn fatal() {
        let s = TestSuite::new("foo");
        let s = s.push(TestCase::new(
            "a",
            "foo",
            &TestResult::error("that's the why"),
            Duration::milliseconds(1000),
        ));
        let s = s.push(TestCase::new(
            "b",
            "foo",
            &TestResult::failure("because"),
            Duration::milliseconds(500),
        ));
        assert_eq!(s.time(), Duration::milliseconds(1500));
        assert_eq!(s.test_count(), 2);
        assert_eq!(s.failure_count(), 1);
        assert_eq!(s.skipped_count(), 0);
        assert_eq!(s.error_count(), 1);
        assert_eq!(s.as_exit_code(), 1);
        assert_eq!(
            s.as_end_str(),
            "Tests run: 2, Failures: 1, Errors: 1, Skipped: 0, Time elapsed: 1.5 sec <<< FAILURE - in foo\na(foo) Time elapsed: 1 <<< ERROR!\n\terror: that's the why\nb(foo) Time elapsed: 0.5 <<< FAILURE!\n\tassertion: because"
                .to_string()
        )
    }
}
