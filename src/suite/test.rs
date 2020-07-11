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

use crate::suite::PlanCommand;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::borrow::Cow;
use std::process::{Command, Stdio};

impl PlanTest {
    pub fn run<'a>(
        &'a self,
        class: &'_ str,
        method: &'_ str,
    ) -> Option<crate::reports::TestCase<'a>> {
        let mut child = match &self.command {
            PlanCommand::Shell(cmd) => {
                if cfg!(target_os = "windows") {
                    let mut command = Command::new("cmd");
                    command.args(&["/C", cmd]);
                    command
                } else {
                    let mut command = Command::new("sh");
                    command.arg("-c").arg(cmd);
                    command
                }
            }
            PlanCommand::Exec(args) => {
                if args.is_empty() {
                    return None;
                }
                let mut child =
                    Command::new(args.get(0).expect("A command to execute has been supplied"));
                child.args(&args[1..]);
                child
            }
        };
        debug!("Forking {}", self.command.display());

        let start = Utc::now();
        let child = match child.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Err(e) => {
                let test_result = crate::reports::TestResult::error(&format!(
                    "The `{}` command failed to start: {:?}",
                    self.command.display(),
                    e
                ));
                return Some(crate::reports::TestCase::new(
                    method,
                    class,
                    &test_result,
                    Utc::now().signed_duration_since(start),
                ));
            }
            Ok(child) => child,
        };
        let output = match child.wait_with_output() {
            Err(e) => {
                let test_result = crate::reports::TestResult::error(&format!(
                    "The `{}` command didn't start: {:?}",
                    self.command.display(),
                    e
                ));
                return Some(crate::reports::TestCase::new(
                    method,
                    class,
                    &test_result,
                    Utc::now().signed_duration_since(start),
                ));
            }
            Ok(status) => status,
        };
        let duration = Utc::now().signed_duration_since(start);
        let success_codes: Vec<i32> = self.success.clone().unwrap_or_else(|| vec![0]);
        let skipped_codes: Vec<i32> = self.skipped.clone().unwrap_or_else(Vec::new);
        let failure_codes: Vec<i32> = self.failure.clone().unwrap_or_else(|| vec![1]);
        let code = output.status.code().unwrap_or(0);
        let message = format!(
            "Terminated with exit code {}, expected {:?}",
            code, success_codes
        );
        let test_result = if success_codes.contains(&code) {
            crate::reports::TestResult::success()
        } else if failure_codes.contains(&code) {
            crate::reports::TestResult::failure(&message)
        } else if skipped_codes.contains(&code) {
            crate::reports::TestResult::skipped(&message)
        } else {
            crate::reports::TestResult::error(&message)
        };
        Some(crate::reports::TestCase::new_with_output(
            method,
            class,
            &test_result,
            Cow::Owned(String::from_utf8_lossy(&output.stdout).to_string()),
            Cow::Owned(String::from_utf8_lossy(&output.stderr).to_string()),
            duration,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanTest {
    /// the command to execute
    #[serde(alias = "cmd")]
    pub command: PlanCommand,
    /// the exit codes to interpret as success
    #[serde(default)]
    pub success: Option<Vec<i32>>,
    /// the exit codes to interpret as failure
    #[serde(default)]
    pub failure: Option<Vec<i32>>,
    /// the exit codes to interpret as skipped
    #[serde(default)]
    pub skipped: Option<Vec<i32>>,
}

#[cfg(test)]
mod tests {
    use crate::reports::TestResult;
    use crate::suite::{PlanCommand, PlanTest};

    #[test]
    fn successful_test() {
        let instance = PlanTest {
            command: PlanCommand::Exec(vec!["echo".to_string(), "hello world".to_string()]),
            success: None,
            failure: None,
            skipped: None,
        };
        let result = instance.run("test.execution", "success").unwrap();
        assert_eq!(result.name(), "success");
        assert_eq!(result.class(), "test.execution");
        assert_eq!(result.result(), &TestResult::success());
        assert_eq!(result.stdout().trim(), "hello world");
    }

    #[test]
    fn failure_test() {
        let instance = PlanTest {
            command: PlanCommand::Shell("exit 3".to_string()),
            success: None,
            failure: Some(vec![3]),
            skipped: None,
        };
        let result = instance.run("test.execution", "failure").unwrap();
        assert_eq!(result.name(), "failure");
        assert_eq!(result.class(), "test.execution");
        assert_eq!(
            result.result(),
            &TestResult::failure("Terminated with exit code 3, expected [0]")
        );
    }

    #[test]
    fn skipped_test() {
        let instance = PlanTest {
            command: PlanCommand::Shell("exit 3".to_string()),
            success: None,
            failure: None,
            skipped: Some(vec![3]),
        };
        let result = instance.run("test.execution", "skipped").unwrap();
        assert_eq!(result.name(), "skipped");
        assert_eq!(result.class(), "test.execution");
        assert_eq!(
            result.result(),
            &TestResult::skipped("Terminated with exit code 3, expected [0]")
        );
    }

    #[test]
    fn error_test() {
        let instance = PlanTest {
            command: PlanCommand::Shell("exit 3".to_string()),
            success: None,
            failure: None,
            skipped: None,
        };
        let result = instance.run("test.execution", "error").unwrap();
        assert_eq!(result.name(), "error");
        assert_eq!(result.class(), "test.execution");
        assert_eq!(
            result.result(),
            &TestResult::error("Terminated with exit code 3, expected [0]")
        );
    }
}
