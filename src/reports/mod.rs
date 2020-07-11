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

mod case;
mod result;
mod suite;
mod transform;
mod xml_util;

pub use case::TestCase;
pub use result::TestResult;
pub use suite::TestSuite;
pub use transform::ReportProcessor;
pub use xml_util::pretty_xml_output;
pub use xml_util::ToWrite;
