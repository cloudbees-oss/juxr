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

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PlanCommand {
    Shell(String),
    Exec(Vec<String>),
}

impl PlanCommand {
    pub fn display(&self) -> String {
        match self {
            PlanCommand::Shell(cmd) => format!("sh -c '{}'", cmd),
            PlanCommand::Exec(args) => args.join(" "),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::suite::PlanCommand;

    #[test]
    fn display_text() {
        assert_eq!(
            PlanCommand::Shell("echo hello world".to_string()).display(),
            "sh -c 'echo hello world'".to_string()
        );
        assert_eq!(
            PlanCommand::Exec(vec!["echo".to_string(), "hello world".to_string()]).display(),
            "echo hello world".to_string()
        );
    }
}
