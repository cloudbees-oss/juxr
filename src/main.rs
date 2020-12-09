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

#[macro_use]
extern crate log;

use std::cell::RefCell;
use std::fs::File;
use std::io::{copy, stderr, stdin, stdout, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::{env, fs, process, thread};

use base64::read::DecoderReader;
use base64::write::EncoderWriter;
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use pretty_env_logger::env_logger::DEFAULT_FILTER_ENV;
use xml::EventWriter;

use juxr::reports::{pretty_xml_output, ReportProcessor, TestSuite};
use juxr::streams::TrimFilterReader;
use juxr::streams::{EmbeddedStreams, Needle};
use juxr::suite;
use juxr::tap::read_tap;

fn main() {
    let args = LocalizedArgs::new();
    let matches = args.get_matches();

    let filters = if matches.is_present("debug") {
        format!("info,{}=debug", env!("CARGO_PKG_NAME"))
    } else {
        env::var(DEFAULT_FILTER_ENV).unwrap_or_else(|_| "info".into())
    };
    pretty_env_logger::formatted_builder()
        .parse_filters(&filters)
        .init();

    let (subcommand, subcommand_args) = matches.subcommand();
    let empty_args = ArgMatches::default();
    let subcommand_args = subcommand_args.unwrap_or(&empty_args);
    process::exit(match subcommand {
        "import" => import(subcommand_args),
        "export" => export(subcommand_args),
        "exec" => exec(subcommand_args),
        "test" => test(subcommand_args),
        "run" => run(subcommand_args),
        "tap" => tap(subcommand_args),
        _ => 1,
    });
}

/// runs a command or parses STDIN for TAP formatted results
fn tap(args: &ArgMatches) -> i32 {
    let dir = output_dir(args);
    let suite = args.value_of("name").expect("Name provided").to_string();
    println!("Running {}", suite);
    let (suite_results, status) = if let Some(command) = args.values_of_lossy("command") {
        let mut child = Command::new(
            command
                .get(0)
                .expect("A command to execute has been supplied"),
        );
        if command.len() > 1 {
            let _ = child.args(&command[1..]);
        };
        debug!("Forking {:?}", command);
        let mut child = match child.stdout(Stdio::piped()).spawn() {
            Err(e) => {
                error!(
                    "The `{}` command failed to start: {:?}",
                    command.join(" "),
                    e
                );
                return 11;
            }
            Ok(child) => child,
        };
        let result = {
            let mut child_stdout = child.stdout.as_mut().unwrap();
            let mut reader = BufReader::new(&mut child_stdout);
            read_tap(&mut reader)
        };
        let status = match child.wait() {
            Err(e) => {
                error!("The `{}`command didn't start: {:?}", command.join(" "), e);
                return 11;
            }
            Ok(status) => status,
        };
        (result, status.code().unwrap_or(0))
    } else {
        let stdin = stdin();
        let mut lock = stdin.lock();
        (read_tap(&mut lock), 0)
    };

    let suite_results = match suite_results {
        Ok(suite_results) => suite_results,
        Err(e) => {
            error!("Could not parse TAP results {:?}", e);
            return 11;
        }
    };

    println!("{}", suite_results.as_end_str());

    let path = dir.join(Path::new(format!("TEST-{}.xml", &suite).as_str()));
    let file = File::create(&path).unwrap();
    if let Err(e) =
        suite_results.write(&mut EventWriter::new_with_config(file, pretty_xml_output()))
    {
        error!("Could not write test results: {:?}", e);
        return 11;
    };
    if args.is_present("ignore_failures") {
        0
    } else if status > 0 {
        status
    } else {
        suite_results.as_exit_code()
    }
}

fn run(args: &ArgMatches) -> i32 {
    let dir = output_dir(args);
    let mut exit_code = 0;
    for suite_filename in args.values_of("suites").unwrap_or_default() {
        let suite_path = Path::new(suite_filename);
        let suite_file = match File::open(suite_path) {
            Ok(f) => f,
            Err(e) => {
                error!(
                    "Could not open test definitions from {}: {:?}",
                    suite_path.display(),
                    e
                );
                exit_code = 1;
                continue;
            }
        };
        let suite_tests = match suite::Plan::from_reader(suite_file) {
            Ok(suite_tests) => suite_tests,
            Err(e) => {
                error!(
                    "Could not read tests from {}: {:?}",
                    suite_path.display(),
                    e
                );
                exit_code = 1;
                continue;
            }
        };
        let suite_name = suite_path.file_stem().unwrap().to_string_lossy();

        let mut suite_results = TestSuite::new(suite_name.as_ref());
        println!("{}", suite_results.as_start_str());
        for (test_name, test) in suite_tests.iter() {
            if let Some(test_case) = test.run(suite_name.as_ref(), test_name) {
                suite_results = suite_results.push(test_case);
            }
        }
        println!("{}", suite_results.as_end_str());
        let path = dir.join(Path::new(format!("TEST-{}.xml", &suite_name).as_str()));
        let file = File::create(&path).unwrap();
        if let Err(e) =
            suite_results.write(&mut EventWriter::new_with_config(file, pretty_xml_output()))
        {
            error!(
                "Could not write test results to {}: {:?}",
                path.display(),
                e
            );
            exit_code = 1;
        };
        if suite_results.as_exit_code() != 0 {
            exit_code = 1
        }
    }
    if args.is_present("ignore_failures") {
        0
    } else {
        exit_code
    }
}

fn output_dir(args: &ArgMatches) -> PathBuf {
    let cwd = env::current_dir()
        .map(|d| d.canonicalize().unwrap_or(d))
        .unwrap_or_default();
    args.value_of_os("directory")
        .map(|s| Path::new(s).to_path_buf())
        .map(|d| if d.is_absolute() { d } else { cwd.join(d) })
        .map(|d| {
            if let Err(e) = fs::create_dir_all(&d) {
                error!("Could not create output directory {}: {:?}", d.display(), e);
            }
            d.canonicalize().unwrap_or(d)
        })
        .unwrap_or(cwd)
}

fn test(args: &ArgMatches) -> i32 {
    let dir = output_dir(&args);
    let test = suite::PlanTest {
        command: suite::PlanCommand::Exec(
            args.values_of("command")
                .expect("A command to execute has been supplied")
                .map(|s| s.to_string())
                .collect(),
        ),
        skipped: args
            .values_of("skipped")
            .map(|v| v.collect::<Vec<&str>>())
            .map(|v| v.iter().flat_map(|c| i32::from_str(c).ok()).collect()),
        success: args
            .values_of("success")
            .map(|v| v.collect::<Vec<&str>>())
            .map(|v| v.iter().flat_map(|c| i32::from_str(c).ok()).collect()),
        failure: args
            .values_of("failure")
            .map(|v| v.collect::<Vec<&str>>())
            .map(|v| v.iter().flat_map(|c| i32::from_str(c).ok()).collect()),
    };
    let name = args.value_of("test").expect("Name provided").to_string();
    let suite = args.value_of("name").expect("Name provided").to_string();
    let mut suite_results = TestSuite::new(suite.as_ref());
    println!("{}", suite_results.as_start_str());
    if let Some(test_case) = test.run(&suite, &name) {
        suite_results = suite_results.push(test_case);
    }
    println!("{}", suite_results.as_end_str());

    let path = dir.join(Path::new(format!("TEST-{}.xml", suite).as_str()));
    let file = File::create(&path).unwrap();
    if let Err(e) =
        suite_results.write(&mut EventWriter::new_with_config(file, pretty_xml_output()))
    {
        error!(
            "Could not write test results to {}: {:?}",
            path.display(),
            e
        );
        return 1;
    };
    if args.is_present("ignore_failures") {
        0
    } else {
        suite_results.as_exit_code()
    }
}

fn exec(args: &ArgMatches) -> i32 {
    let redirect_err_to_out = args.is_present("redirect_err_to_out");
    let command = args
        .values_of("command")
        .expect("A command to execute has been supplied");
    let command: Vec<&str> = command.collect();
    let mut child = Command::new(
        command
            .get(0)
            .expect("A command to execute has been supplied"),
    );
    if command.len() > 1 {
        let _ = child.args(&command[1..]);
    };
    debug!("Forking {:?}", command);
    // need to pipe output so that we can flush line by line
    let mut child = match child.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Err(e) => {
            error!(
                "The `{}` command failed to start: {:?}",
                command.join(" "),
                e
            );
            return 11;
        }
        Ok(child) => child,
    };
    let mut child_stdout = BufReader::new(child.stdout.take().unwrap());
    let out_piper = thread::spawn(move || {
        let out = stdout();
        let mut buf = vec![];
        while let Ok(count) = child_stdout.read_until(b'\n', &mut buf) {
            if count > 0 {
                let mut lock = out.lock();
                lock.write(&buf[0..count]).unwrap_or_default();
                buf.clear();
                lock.flush().unwrap_or_default();
            } else {
                break;
            }
        }
    });
    let mut child_stderr = BufReader::new(child.stderr.take().unwrap());
    let err_piper = thread::spawn(move || {
        if redirect_err_to_out {
            let out = stdout();
            let mut buf = vec![];
            while let Ok(count) = child_stderr.read_until(b'\n', &mut buf) {
                if count > 0 {
                    let mut lock = out.lock();
                    lock.write(&buf[0..count]).unwrap_or_default();
                    buf.clear();
                    lock.flush().unwrap_or_default();
                } else {
                    break;
                }
            }
        } else {
            let out = stderr();
            let mut buf = vec![];
            while let Ok(count) = child_stderr.read_until(b'\n', &mut buf) {
                if count > 0 {
                    let mut lock = out.lock();
                    lock.write(&buf[0..count]).unwrap_or_default();
                    buf.clear();
                    lock.flush().unwrap_or_default();
                } else {
                    break;
                }
            }
        }
    });
    let status = match child.wait_with_output() {
        Err(e) => {
            error!("The `{}`command didn't start: {:?}", command.join(" "), e);
            return 11;
        }
        Ok(status) => status,
    };
    // ensure all output has been flushed to stdout/stderr
    out_piper.join().unwrap_or_default();
    err_piper.join().unwrap_or_default();
    // now we should be safe to output our own
    let out = stdout();
    let mut lock = out.lock();
    debug!("{:?} finished with status {:?}", command, status);
    export_reports(args, &mut lock).unwrap_or_else(|e| {
        error!("Could not export: {:?}", e);
        process::exit(1)
    });
    if args.is_present("ignore_failures") {
        0
    } else {
        status.status.code().unwrap_or_default()
    }
}

fn export(args: &ArgMatches) -> i32 {
    export_reports(args, &mut stdout().lock()).unwrap_or_else(|e| {
        error!("Could not export: {:?}", e);
        process::exit(1)
    });
    0
}

fn import(args: &ArgMatches) -> i32 {
    let dir = output_dir(&args);
    let processor = report_processor(args);
    let success = RefCell::new(Some(true));
    EmbeddedStreams::new(stdin().lock(), &mut stdout().lock()).for_each(|stream| {
        let mut success_mut = success.borrow_mut();
        let name = stream.name();
        let kind = stream.kind().unwrap_or_default();

        let file_name = dir.join(Path::new(&name.strip_prefix('/').unwrap_or(&name)));
        debug!("Decoding {}", file_name.to_string_lossy());
        if let Some(parent) = file_name.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                error!(
                    "Could not create directory {}: {:?}",
                    parent.to_string_lossy(),
                    e
                );
                success_mut.replace(false);
            }
        }

        match File::create(file_name) {
            Ok(file) => {
                let mut writer = BufWriter::new(file);
                let result = {
                    let mut filter = TrimFilterReader::new(stream);
                    let mut decoder = DecoderReader::new(&mut filter, base64::STANDARD);
                    match kind.as_str() {
                        "junit-test-report" => processor
                            .reset()
                            .attachment_prefix(&dir.to_string_lossy())
                            .process(&mut decoder, &mut writer),
                        _ => copy(&mut decoder, &mut writer)
                            .map(|_| ())
                            .map_err(|e| e.into()),
                    }
                };
                if let Err(e) = result {
                    error!("Could not complete writing to file {}: {:?}", name, e);
                    success_mut.replace(false);
                }
            }
            Err(e) => {
                error!("Could not create file {}: {:?}", name, e);
                success_mut.replace(false);
            }
        }
    });
    if success.borrow().unwrap_or_default() {
        0
    } else {
        1
    }
}

struct LocalizedArgs {
    secrets: String,
    reports: String,
    files: String,
    test_suite_name_prefix: String,
    test_suite_name_suffix: String,
    test_case_name_prefix: String,
    test_case_name_suffix: String,
    test_case_class_prefix: String,
    test_case_class_suffix: String,
    skip_export: String,
}

impl LocalizedArgs {
    fn new() -> LocalizedArgs {
        let prefix = env!("CARGO_PKG_NAME")
            .to_uppercase()
            .replace(|c| !((c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')), "_");
        LocalizedArgs {
            reports: format!("{}_REPORTS", prefix),
            secrets: format!("{}_SECRETS", prefix),
            files: format!("{}_FILES", prefix),
            test_suite_name_prefix: format!("{}_SUITE_PREFIX", prefix),
            test_suite_name_suffix: format!("{}_SUITE_SUFFIX", prefix),
            test_case_name_prefix: format!("{}_NAME_PREFIX", prefix),
            test_case_name_suffix: format!("{}_NAME_SUFFIX", prefix),
            test_case_class_prefix: format!("{}_CLASS_PREFIX", prefix),
            test_case_class_suffix: format!("{}_CLASS_SUFFIX", prefix),
            skip_export: format!("{}_SKIP_EXPORT", prefix),
        }
    }

    fn add_export_args<'a, 'b>(&'a self, app: App<'a, 'b>) -> App<'a, 'b> {
        self.add_rewrite_report_args(app)
            .arg(
                Arg::with_name("reports")
                    .long("reports")
                    .short("r")
                    .env(&self.reports)
                    .takes_value(true)
                    .multiple(true)
                    .help("The JUnit XML report file(s) to export, supports * and ** style globs"),
            )
            .arg(
                Arg::with_name("files")
                    .long("files")
                    .env(&self.files)
                    .takes_value(true)
                    .multiple(true)
                    .help("Additional files to export, supports * and ** style globs"),
            )
    }

    fn add_rewrite_report_args<'a, 'b>(&'a self, app: App<'a, 'b>) -> App<'a, 'b> {
        app.arg(
            Arg::with_name("test_suite_prefix")
                .long("test-suite-prefix")
                .takes_value(true)
                .env(&self.test_suite_name_prefix)
                .help("A string to prepend to each test suite name")
        )
            .arg(
                Arg::with_name("test_suite_suffix")
                    .long("test-suite-suffix")
                    .takes_value(true)
                    .env(&self.test_suite_name_suffix)
                    .help("A string to append to each test suite name")
            )
            .arg(
                Arg::with_name("test_name_prefix")
                    .long("test-name-prefix")
                    .takes_value(true)
                    .env(&self.test_case_name_prefix)
                    .help("A string to prepend to each test case name")
            )
            .arg(
                Arg::with_name("test_name_suffix")
                    .long("test-name-suffix")
                    .takes_value(true)
                    .env(&self.test_case_name_suffix)
                    .help("A string to append to each test case name")
            )
            .arg(
                Arg::with_name("test_class_prefix")
                    .long("test-class-prefix")
                    .takes_value(true)
                    .env(&self.test_case_class_prefix)
                    .help("A string to prepend to each test case class name")
            )
            .arg(
                Arg::with_name("test_class_suffix")
                    .long("test-class-suffix")
                    .takes_value(true)
                    .env(&self.test_case_class_suffix)
                    .help("A string to append to each test case class name")
            )
            .arg(
                Arg::with_name("secret")
                    .long("secret")
                    .short("s")
                    .takes_value(true)
                    .multiple(true)
                    .help("Name of an environment variable with a value that should be redacted from the reports")
            )
            .arg(
                Arg::with_name("secrets")
                    .long("secrets")
                    .env(&self.secrets)
                    .help("A comma separated list of environment variable names with values that should be redacted from the report")
            )
            .arg(
                Arg::with_name("skip_export")
                    .long("skip-export")
                    .env(&self.skip_export)
                    .default_value("false")
                    .help("Set to `true` to skip exporting, for use in scripts / containers where you do not always want to export reports")
            )
    }

    fn get_matches(&self) -> ArgMatches {
        App::new(env!("CARGO_PKG_NAME"))
            .version(env!("CARGO_PKG_VERSION"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .setting(AppSettings::ColorAuto)
            .setting(AppSettings::SubcommandRequiredElseHelp)
            .arg(
                Arg::with_name("debug")
                    .short("d")
                    .long("debug")
                    .takes_value(false)
                    .help("Turn on debug logging"),
            )
            .subcommand(
                SubCommand::with_name("import")
                    .about("Imports JUnit XML Reports and attachments from STDIN")
                    .arg(
                        Arg::with_name("directory")
                            .takes_value(true)
                            .short("o")
                            .long("output")
                            .default_value(".")
                            .help("Directory in which to write imported files"),
                    ),
            )
            .subcommand(
                self.add_export_args(SubCommand::with_name("export"))
                    .about("Export JUnit XML Reports (and any referenced attachments) to STDOUT"),
            )
            .subcommand(
                self.add_export_args(SubCommand::with_name("exec"))
                    .arg(
                        Arg::with_name("redirect_err_to_out")
                            .long("redirect-err-to-out")
                            .takes_value(false)
                            .help("Redirects the child processes STDERR to STDOUT, useful in cases where buffering is corrupting JUXR's export")
                    )
                    .about("Runs a command that generates JUnit XML Reports and exports them (and any referenced attachments) to STDOUT before propagating the invoked command's exit code")
                    .arg(
                        Arg::with_name("command")
                            .last(true)
                            .multiple(true)
                            .required(true)
                            .help("The command to execute"),
                    ),
            )
            .subcommand(
                SubCommand::with_name("run")
                    .about("Runs a basic set of tests as expressed in a simplified YAML format and \
                    captures their results as a JUnit XML format test report.")
                    .arg(
                        Arg::with_name("directory")
                            .takes_value(true)
                            .short("o")
                            .long("output")
                            .default_value(".")
                            .help("Directory in which to write imported files"),
                    )
                    .arg(
                        Arg::with_name("suites")
                            .multiple(true)
                            .required(true)
                            .help("YAML test suite to run and capture the results in JUnit XML format")
                    )
                    .arg(
                        Arg::with_name("ignore_failures")
                            .long("ignore-failures")
                            .help("Test failures/errors will not affect the exit code")
                    )
                ,
            )
            .subcommand(
                SubCommand::with_name("test")
                    .about("Runs a single command as a test and captures the result in JUnit XML format")
                    .arg(
                        Arg::with_name("command")
                            .last(true)
                            .multiple(true)
                            .required(true)
                            .help("The command to execute"),
                    )
                    .arg(
                        Arg::with_name("success")
                            .long("success")
                            .default_value("0")
                            .value_delimiter(",")
                            .value_name("CODE")
                            .help("A comma separated list of exit codes of the command indicating a successful test result")
                    )
                    .arg(
                        Arg::with_name("failure")
                            .long("failure")
                            .default_value("1")
                            .value_delimiter(",")
                            .value_name("CODE")
                            .help("A comma separated list of exit codes of the command indicating a failed test result")
                    )
                    .arg(
                        Arg::with_name("skipped")
                            .long("skipped")
                            .value_name("CODE")
                            .value_delimiter(",")
                            .help("A comma separated list of exit codes of the command indicating skipped test")
                    )
                    .arg(
                        Arg::with_name("test")
                            .short("t")
                            .long("test")
                            .takes_value(true)
                            .value_name("NAME")
                            .required(true)
                            .help("The name of the test case")
                    )
                    .arg(
                        Arg::with_name("name")
                            .short("n")
                            .long("name")
                            .takes_value(true)
                            .value_name("NAME")
                            .required(true)
                            .help("The name of the test suite")
                    )
                    .arg(
                        Arg::with_name("directory")
                            .takes_value(true)
                            .short("o")
                            .long("output")
                            .default_value(".")
                            .help("Directory in which to write the test result")
                    )
                    .arg(
                        Arg::with_name("ignore_failures")
                            .long("ignore-failures")
                            .help("Test failures/errors will not affect the exit code")
                    )
            )
            .subcommand(SubCommand::with_name("tap")
                .about("Parses TAP formatted results into JUnit XML Report format. \
                If no command is specified then STDIN will be parsed for the TAP formatted test \
                report otherwise the supplied command will be run and its output parsed as a TAP \
                formatted test report")
                .arg(
                    Arg::with_name("directory")
                        .takes_value(true)
                        .short("o")
                        .long("output")
                        .default_value(".")
                        .help("Directory in which to write the test result")
                )
                .arg(
                    Arg::with_name("name")
                        .short("n")
                        .long("name")
                        .takes_value(true)
                        .value_name("NAME")
                        .required(true)
                        .help("The name of the test suite")
                )
                .arg(
                    Arg::with_name("command")
                        .last(true)
                        .multiple(true)
                        .help("The command to execute, otherwise input will be read from STDIN"),
                )
                .arg(
                    Arg::with_name("ignore_failures")
                        .long("ignore-failures")
                        .help("Test failures/errors will not affect the exit code")
                )
            )
            .get_matches()
    }
}

fn report_processor(args: &ArgMatches) -> ReportProcessor {
    let mut processor = ReportProcessor::new();
    if let Some(value) = args.value_of("test_suite_prefix") {
        processor = processor.test_suite_name_prefix(value);
    }
    if let Some(value) = args.value_of("test_suite_suffix") {
        processor = processor.test_suite_name_suffix(value);
    }
    if let Some(value) = args.value_of("test_name_prefix") {
        processor = processor.test_case_name_prefix(value);
    }
    if let Some(value) = args.value_of("test_name_suffix") {
        processor = processor.test_case_name_suffix(value);
    }
    if let Some(value) = args.value_of("test_class_prefix") {
        processor = processor.test_case_class_prefix(value);
    }
    if let Some(value) = args.value_of("test_class_suffix") {
        processor = processor.test_case_class_suffix(value);
    }
    if let Some(secrets) = args.value_of("secrets") {
        for secret in secrets.split(',') {
            if let Some(value) = env::var_os(secret) {
                debug!(
                    "Redacting value of environment variable {} from reports",
                    secret
                );
                processor = processor.secret(&value.to_string_lossy());
            }
        }
    }
    for secret in args.values_of("secret").unwrap_or_default() {
        if let Some(value) = env::var_os(secret) {
            debug!(
                "Redacting value of environment variable {} from reports",
                secret
            );
            processor = processor.secret(&value.to_string_lossy());
        }
    }
    processor
}

fn export_reports<W: Write>(args: &ArgMatches, mut out: &mut W) -> anyhow::Result<()> {
    if let Some(skip) = args.value_of_lossy("skip_export") {
        let skip = skip.to_lowercase().trim().to_string();
        match skip.as_ref() {
            "true" | "skip" | "1" | "y" | "yes" | "t" => {
                info!("Exporting skipped");
                return Ok(());
            }
            _ => (),
        }
    }
    let processor = report_processor(args);
    for report_glob in args.values_of("reports").unwrap_or_default() {
        for report in globwalk::glob(report_glob).unwrap() {
            if let Ok(report) = report {
                let file = report.path().canonicalize().unwrap();
                let file = if let Ok(f) = file
                    .clone()
                    .strip_prefix(env::current_dir().unwrap_or_default())
                {
                    f.to_path_buf()
                } else {
                    file
                };
                let mut processor = processor.reset();
                debug!("Exporting report: {}", report.path().to_string_lossy());
                {
                    let needle =
                        Needle::new_with_kind(&file.to_string_lossy(), "junit-test-report")
                            .to_string();
                    out.write_all(needle.as_bytes())?;
                    let mut reader = BufReader::new(File::open(file).unwrap());
                    let result = {
                        let mut writer = BufWriter::new(&mut out);
                        let mut encoder = EncoderWriter::new(&mut writer, base64::STANDARD);
                        let result = processor.process(&mut reader, &mut encoder);
                        encoder.finish()?;
                        result
                    };
                    out.write_all(needle.as_bytes())?;
                    if let Err(e) = result {
                        error!(
                            "Could not complete parsing report {}: {:?}",
                            report.path().to_string_lossy(),
                            e
                        );
                    }
                }
                for attachment in processor.attachments() {
                    if let Ok(file) = File::open(attachment) {
                        let needle = Needle::new(&attachment).to_string();
                        out.write_all(needle.as_bytes())?;
                        let mut reader = BufReader::new(file);
                        {
                            let mut writer = EncoderWriter::new(&mut out, base64::STANDARD);
                            copy(&mut reader, &mut writer)?;
                        }
                        out.write_all(needle.as_bytes())?;
                    }
                }
            }
        }
    }
    for file_glob in args.values_of("files").unwrap_or_default() {
        for file in globwalk::glob(file_glob).unwrap() {
            if let Ok(file) = file {
                let path = file.path().canonicalize().unwrap();
                let path = if let Ok(f) = path
                    .clone()
                    .strip_prefix(env::current_dir().unwrap_or_default())
                {
                    f.to_path_buf()
                } else {
                    path
                };
                let file_name = &path.to_string_lossy();
                debug!("Exporting file: {}", file_name);
                if let Ok(file) = File::open(path.clone()) {
                    let needle = Needle::new(&file_name).to_string();
                    out.write_all(needle.as_bytes())?;
                    let mut reader = BufReader::new(file);
                    {
                        let mut writer = EncoderWriter::new(&mut out, base64::STANDARD);
                        copy(&mut reader, &mut writer)?;
                    }
                    out.write_all(needle.as_bytes())?;
                }
            }
        }
    }
    Ok(())
}
