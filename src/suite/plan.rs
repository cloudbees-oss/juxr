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

use crate::suite::{PlanCommand, PlanTest};
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Plan {
    tests: BTreeMap<String, PlanTest>,
}

impl Plan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_reader<R>(rdr: R) -> serde_yaml::Result<Self>
    where
        R: io::Read,
    {
        serde_yaml::from_reader(rdr).map(Self::from_map)
    }

    fn from_map(p: BTreeMap<String, TestCase>) -> Self {
        let mut tests = BTreeMap::new();
        for (k, v) in p {
            tests.insert(k, v.into());
        }
        Self { tests }
    }

    pub fn to_string(&self) -> serde_yaml::Result<String> {
        serde_yaml::to_string(&self.tests)
    }

    pub fn iter(&self) -> std::collections::btree_map::Iter<'_, String, PlanTest> {
        self.tests.iter()
    }

    pub fn get(&self, name: &str) -> Option<&PlanTest> {
        self.tests.get(name)
    }

    pub fn insert(&mut self, name: &str, test: PlanTest) -> Option<PlanTest> {
        self.tests.insert(name.to_string(), test)
    }
}

impl FromStr for Plan {
    type Err = serde_yaml::Error;

    fn from_str(s: &str) -> serde_yaml::Result<Self> {
        serde_yaml::from_str(s).map(Self::from_map)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
enum TestCase {
    Shell(String),
    Exec(Vec<String>),
    Detail(TestPlan),
}

impl Into<PlanTest> for TestCase {
    fn into(self) -> PlanTest {
        match self {
            Self::Shell(cmd) => PlanTest {
                command: PlanCommand::Shell(cmd),
                success: None,
                failure: None,
                skipped: None,
            },
            Self::Exec(args) => PlanTest {
                command: PlanCommand::Exec(args),
                success: None,
                failure: None,
                skipped: None,
            },
            Self::Detail(detail) => PlanTest {
                command: detail.command,
                success: detail.success.map(|v| v.into()),
                failure: detail.failure.map(|v| v.into()),
                skipped: detail.skipped.map(|v| v.into()),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
enum TestExit {
    Single(i32),
    Multiple(Vec<i32>),
}

impl Into<Vec<i32>> for TestExit {
    fn into(self) -> Vec<i32> {
        match self {
            Self::Single(n) => vec![n],
            Self::Multiple(v) => v,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestPlan {
    /// the command to execute
    #[serde(alias = "cmd")]
    pub command: PlanCommand,
    /// the exit codes to interpret as success
    #[serde(default)]
    pub success: Option<TestExit>,
    /// the exit codes to interpret as failure
    #[serde(default)]
    pub failure: Option<TestExit>,
    /// the exit codes to interpret as skipped
    #[serde(default)]
    pub skipped: Option<TestExit>,
}

#[cfg(test)]
mod tests {
    use crate::suite::{Plan, PlanCommand, PlanTest};
    use std::io::Cursor;
    use std::str::FromStr;

    #[test]
    fn create_empty() {
        let plan = Plan::new();
        assert_eq!(plan.tests.len(), 0);
    }

    #[test]
    fn create() {
        let mut plan = Plan::new();
        plan.insert(
            "foo",
            PlanTest {
                command: PlanCommand::Shell("echo truth".to_string()),
                success: None,
                failure: None,
                skipped: None,
            },
        );
        plan.insert(
            "bar",
            PlanTest {
                command: PlanCommand::Exec(vec!["echo".to_string(), "truth".to_string()]),
                success: None,
                failure: None,
                skipped: None,
            },
        );
        assert_eq!(plan.tests.len(), 2);
        assert_eq!(
            plan.get("foo"),
            Some(&PlanTest {
                command: PlanCommand::Shell("echo truth".to_string()),
                success: None,
                failure: None,
                skipped: None
            })
        );
        assert_eq!(
            plan.get("bar"),
            Some(&PlanTest {
                command: PlanCommand::Exec(vec!["echo".to_string(), "truth".to_string()]),
                success: None,
                failure: None,
                skipped: None
            })
        )
    }

    #[test]
    fn parse_with_reader() {
        let input = include_str!("../../test/plan/basic.yaml");
        let plan = Plan::from_reader(Cursor::new(&input.as_bytes())).unwrap();
        assert_eq!(plan.tests.len(), 2);
        assert_eq!(
            plan.get("foo"),
            Some(&PlanTest {
                command: PlanCommand::Shell("echo truth".to_string()),
                success: None,
                failure: None,
                skipped: None
            })
        );
        assert_eq!(
            plan.get("bar"),
            Some(&PlanTest {
                command: PlanCommand::Exec(vec!["echo".to_string(), "truth".to_string()]),
                success: None,
                failure: None,
                skipped: None
            })
        )
    }

    #[test]
    fn parse_short_form() {
        let plan = Plan::from_str(include_str!("../../test/plan/basic.yaml")).unwrap();
        assert_eq!(plan.tests.len(), 2);
        assert_eq!(
            plan.get("foo"),
            Some(&PlanTest {
                command: PlanCommand::Shell("echo truth".to_string()),
                success: None,
                failure: None,
                skipped: None
            })
        );
        assert_eq!(
            plan.get("bar"),
            Some(&PlanTest {
                command: PlanCommand::Exec(vec!["echo".to_string(), "truth".to_string()]),
                success: None,
                failure: None,
                skipped: None
            })
        )
    }

    #[test]
    fn parse_long_form() {
        let plan = Plan::from_str(include_str!("../../test/plan/full.yaml")).unwrap();
        assert_eq!(plan.tests.len(), 2);
        assert_eq!(
            plan.get("foo"),
            Some(&PlanTest {
                command: PlanCommand::Shell("echo truth".to_string()),
                success: Some(vec![0]),
                failure: Some(vec![1]),
                skipped: Some(vec![2])
            })
        );
        assert_eq!(
            plan.get("bar"),
            Some(&PlanTest {
                command: PlanCommand::Exec(vec!["echo".to_string(), "truth".to_string()]),
                success: Some(vec![0]),
                failure: Some(vec![1]),
                skipped: Some(vec![2])
            })
        )
    }
}
