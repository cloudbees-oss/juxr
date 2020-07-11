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

use crate::reports::{TestCase, TestResult, TestSuite};
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use std::borrow::Cow;
use std::io::BufRead;
use std::str::FromStr;

struct TapTestResult {
    result: String,
    number: usize,
    name: Option<String>,
    directive: Option<(String, Option<String>)>,
}

pub fn read_tap<'a, R: BufRead>(input: &'_ mut R) -> anyhow::Result<TestSuite<'a>> {
    let ver = Regex::new(r"^TAP version (?P<version>\d+)$").unwrap();
    let plan = Regex::new(r"^1\.\.(?P<count>\d+)(\s+#.*)?$").unwrap();
    let test = Regex::new(
        r"^(?P<result>(not )?ok)(\s+(?P<number>[0-9][0-9]*))?(\s+(?P<name>[^0-9 ][^#]*))?(#\s*(?P<directive>\S+)\s+(?P<message>.*)?)?$",
    ).unwrap();
    let bail = Regex::new(r"^Bail out!\s*(?P<description>.*)?$").unwrap();
    let diag = Regex::new(r"^#\s?(?P<line>.*)").unwrap();
    let yaml_start = Regex::new(r"^(?P<indent>\s+)---").unwrap();
    let yaml_end = Regex::new(r"^(?P<indent>\s+)\.\.\.").unwrap();

    let mut test_version: Option<usize> = None;
    let mut test_plan: Option<usize> = None;
    let mut test_case: Option<TapTestResult> = None;
    let mut test_output: Vec<String> = Vec::new();
    let mut test_number: usize = 0;

    let mut suite_results = TestSuite::new("tap");
    let mut test_start: DateTime<Utc> = Utc::now();
    let mut yaml_indent: Option<String> = None;

    for line in input.lines().flat_map(|l| l.ok()) {
        if test_version.is_none() {
            // first line should be version if newer version than 12
            if let Some(cap) = ver.captures(&line) {
                let v = usize::from_str(cap.name("version").unwrap().as_str())
                    .expect("only digits should be a valid number");
                if v < 13 {
                    return Err(anyhow::anyhow!("TAP version specified as {}. When specified, the TAP version must be at least 13", v));
                }
                test_version = Some(v);
                continue;
            } else {
                // no version specified means version 12
                test_version = Some(12);
            }
        }
        if let Some(indent) = &yaml_indent {
            if let Some(cap) = yaml_end.captures(&line) {
                if indent == cap.name("indent").unwrap().as_str() {
                    // this is the matching end
                    yaml_indent = None;
                    continue;
                }
            }
            if line.starts_with(indent) {
                test_output.push((&line[indent.len()..]).to_string());
                continue;
            } else {
                yaml_indent = None;
            }
        }

        if let Some(cap) = plan.captures(&line) {
            if test_plan.is_some() {
                return Err(anyhow::anyhow!(
                    "More than one test plan in the supplied input"
                ));
            }
            let test_count = usize::from_str(cap.name("count").unwrap().as_str())
                .expect("only digits should be a valid version number");
            test_plan = Some(test_count);
            if test_number > 0 {
                // the plan is at the end
                while test_number < test_count {
                    suite_results = suite_results.push(TestCase::new(
                        &format!("test {}", test_number),
                        "tap",
                        &TestResult::failure("missing"),
                        Duration::milliseconds(0),
                    ));
                    test_number += 1;
                }
                break;
            }
            test_start = Utc::now();
        } else if let Some(cap) = test.captures(&line) {
            if let Some(TapTestResult {
                result,
                number,
                name,
                directive,
            }) = test_case.take()
            {
                // record the previous test result
                let case = to_test_case(&test_output, test_start, result, number, name, directive);
                suite_results = suite_results.push(case);
            }
            // walk up any missing test numbers as failed

            test_number += 1;
            let result = cap.name("result").map(|m| m.as_str().to_string()).unwrap();
            let number = cap
                .name("number")
                .map(|m| usize::from_str(m.as_str()).unwrap())
                .unwrap_or(test_number);
            while test_number < number {
                suite_results = suite_results.push(TestCase::new(
                    &format!("test {}", test_number),
                    "tap",
                    &TestResult::failure("missing"),
                    Duration::milliseconds(0),
                ));
                test_number += 1;
            }
            let name = cap.name("name").map(|m| m.as_str().to_string());
            let directive = cap
                .name("directive")
                .map(|m| m.as_str().to_string().to_uppercase())
                .map(|d| (d, cap.name("message").map(|m| m.as_str().to_string())));
            test_case.replace(TapTestResult {
                result,
                number,
                name,
                directive,
            });
            test_output.clear();
            test_start = Utc::now();
        } else if bail.is_match(&line) {
            break;
        } else if let Some(cap) = diag.captures(&line) {
            test_output.push(cap.name("line").unwrap().as_str().to_string());
        } else if let Some(cap) = yaml_start.captures(&line) {
            yaml_indent = Some(cap.name("indent").unwrap().as_str().to_string());
            test_output.push("---".to_string());
        } else {
            // unknown
        }
    }
    if let Some(TapTestResult {
        result,
        number,
        name,
        directive,
    }) = test_case.take()
    {
        // record the previous test result
        let case = to_test_case(&test_output, test_start, result, number, name, directive);
        suite_results = suite_results.push(case);
    }
    if let Some(test_count) = test_plan {
        while test_number < test_count {
            suite_results = suite_results.push(TestCase::new(
                &format!("test {}", test_number),
                "tap",
                &TestResult::failure("missing"),
                Duration::milliseconds(0),
            ));
            test_number += 1;
        }
    }
    Ok(suite_results)
}

fn to_test_case<'a>(
    test_output: &'_ [String],
    test_start: DateTime<Utc>,
    result: String,
    number: usize,
    name: Option<String>,
    directive: Option<(String, Option<String>)>,
) -> TestCase<'a> {
    let test_result = match result.as_str() {
        "ok" => match directive {
            None => TestResult::success(),
            Some(d) => {
                if &d.0 == "SKIP" {
                    TestResult::skipped(&d.1.unwrap_or_else(|| "".to_string()))
                } else if &d.0 == "TODO" {
                    TestResult::failure(&d.1.unwrap_or_else(|| "".to_string()))
                } else {
                    TestResult::success()
                }
            }
        },
        "not ok" => match directive {
            None => TestResult::failure(""),
            Some(d) => {
                if &d.0 == "SKIP" {
                    TestResult::skipped(&d.1.unwrap_or_else(|| "".to_string()))
                } else if &d.0 == "TODO" {
                    TestResult::success()
                } else {
                    TestResult::failure(&d.1.unwrap_or_else(|| "".to_string()))
                }
            }
        },
        _ => TestResult::error("unexpected test result"),
    };
    let name = name.unwrap_or_else(|| format!("test {}", number));
    TestCase::new_with_output(
        &name,
        "tap",
        &test_result,
        Cow::Owned(test_output.join("\n")),
        Cow::Borrowed(""),
        Utc::now().signed_duration_since(test_start),
    )
}

#[cfg(test)]
mod tests {
    use crate::tap::read_tap;
    use std::io::{BufReader, Cursor};

    #[test]
    fn tap_spec_13_common_example() {
        let input = include_str!("../../test/tap/13/common.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 6);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_missing() {
        let input = include_str!("../../test/tap/13/missing.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 6);
        assert_eq!(result.failure_count(), 4);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_trailing_output() {
        let input = include_str!("../../test/tap/13/trailing-output.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 1);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_unknown_example() {
        let input = include_str!("../../test/tap/13/unknown.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 7);
        assert_eq!(result.failure_count(), 2);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_unknown9() {
        let input = include_str!("../../test/tap/13/unknown9.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 9);
        assert_eq!(result.failure_count(), 4);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_giveup_example() {
        let input = include_str!("../../test/tap/13/giveup.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 573);
        assert_eq!(result.failure_count(), 573);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_skip_some_example() {
        let input = include_str!("../../test/tap/13/skip-some.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 5);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 4);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_skip_all_example() {
        let input = include_str!("../../test/tap/13/skip-all.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 0);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_todos_example() {
        let input = include_str!("../../test/tap/13/todos.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 4);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_liberties_example() {
        let input = include_str!("../../test/tap/13/liberties.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 9);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_13_yaml_no_end_example() {
        let input = include_str!("../../test/tap/13/yaml-no-end.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 9);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_common_example() {
        let input = include_str!("../../test/tap/12/common.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 6);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_missing() {
        let input = include_str!("../../test/tap/12/missing.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 6);
        assert_eq!(result.failure_count(), 4);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_trailing_output() {
        let input = include_str!("../../test/tap/12/trailing-output.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 1);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_unknown_example() {
        let input = include_str!("../../test/tap/12/unknown.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 7);
        assert_eq!(result.failure_count(), 2);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_unknown9() {
        let input = include_str!("../../test/tap/12/unknown9.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 9);
        assert_eq!(result.failure_count(), 4);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_giveup_example() {
        let input = include_str!("../../test/tap/12/giveup.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 573);
        assert_eq!(result.failure_count(), 573);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_skip_some_example() {
        let input = include_str!("../../test/tap/12/skip-some.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 5);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 4);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_skip_all_example() {
        let input = include_str!("../../test/tap/12/skip-all.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 0);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_todos_example() {
        let input = include_str!("../../test/tap/12/todos.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 4);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_spec_12_liberties_example() {
        let input = include_str!("../../test/tap/12/liberties.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.test_count(), 9);
        assert_eq!(result.failure_count(), 0);
        assert_eq!(result.skipped_count(), 0);
        assert_eq!(result.error_count(), 0);
    }

    #[test]
    fn tap_invalid_two_plans() {
        let input = include_str!("../../test/tap/invalid/two-plans.txt");
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_err(), true);
    }

    #[test]
    fn tap_invalid_version() {
        let input = "TAP version 12\n";
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let result = read_tap(&mut reader);
        assert_eq!(result.is_ok(), false);
    }
}
