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

/// Represents the result of a test
#[derive(Debug, Clone, PartialEq)]
pub enum TestResult<'a> {
    Success,
    Failure {
        type_: Cow<'a, str>,
        message: Cow<'a, str>,
    },
    Skipped {
        message: Cow<'a, str>,
    },
    Error {
        type_: Cow<'a, str>,
        message: Cow<'a, str>,
    },
}

impl<'a> TestResult<'a> {
    /// creates a successful test result
    pub fn success() -> TestResult<'a> {
        TestResult::Success
    }

    /// creates a failed test result
    pub fn failure(message: &'_ str) -> TestResult<'a> {
        TestResult::Failure {
            type_: Cow::Borrowed("assertion"),
            message: Cow::Owned(message.to_string()),
        }
    }

    /// creates an unexpected error test result
    pub fn error(message: &'_ str) -> TestResult<'a> {
        TestResult::Error {
            type_: Cow::Borrowed("error"),
            message: Cow::Owned(message.to_string()),
        }
    }

    /// creates a skipped test result
    pub fn skipped(message: &'_ str) -> TestResult<'a> {
        TestResult::Skipped {
            message: Cow::Owned(message.to_string()),
        }
    }

    /// extracts the message from the test result
    pub fn message(&'a self) -> Option<&'a str> {
        match &self {
            TestResult::Success => None,
            TestResult::Failure { message, .. }
            | TestResult::Skipped { message }
            | TestResult::Error { message, .. } => Some(message.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::reports::TestResult;

    #[test]
    fn extract_message() {
        let r = TestResult::success();
        assert_eq!(r.message(), None);
        let r = TestResult::skipped("just because");
        assert_eq!(r.message(), Some("just because"));
        let r = TestResult::failure("just because");
        assert_eq!(r.message(), Some("just because"));
        let r = TestResult::error("just because");
        assert_eq!(r.message(), Some("just because"));
    }
}
