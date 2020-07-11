# JUnit XML Reporting Toolkit

A command line tool for helping manage JUnit XML formatted reports.

![Release](https://github.com/cloudbees-oss/juxr/workflows/Release/badge.svg) ![Test](https://github.com/cloudbees-oss/juxr/workflows/Test/badge.svg) [![Crates.io](https://img.shields.io/crates/v/juxr.svg?maxAge=2592000)](https://crates.io/crates/juxr) [![Crates.io](https://img.shields.io/crates/d/juxr.svg?maxAge=2592000)](https://crates.io/crates/juxr) 

## Installation

To get the toolkit locally just use [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):

```
cargo install juxr
```              

*NOTE:* Unsigned binaries built by GitHub Actions are provided for convenience and are available in [Releases](https://github.com/cloudbees-oss/juxr/releases).
The recommended installation path is to build from source using `cargo install juxr`.

To get the toolkit for use in Docker containers just copy it from the Docker image:

```Dockerfile
FROM docker.pkg.github.com/cloudbees-oss/juxr:latest AS juxr
# Just to grab the juxr binary, then build your image as normal

FROM your-base-image             
# ...

# Copy in the binary
COPY --from=juxr /usr/local/bin/juxr /usr/local/bin/juxr

# ...
``` 

You can also use the Docker image for running the toolkit, though this is not recommended for sub-commands other than `import` and `export`.

## Extract reports over Standard I/O

> As a developer, I have some tests running in a temporary Kubernetes pod and I need to exract the test results and any associated [attachments](https://plugins.jenkins.io/junit-attachments/)

The toolkit provides three sub-commands to assist with this.

Firstly there is the `export` subcommand. You run this command in your pod and it will pipe the reports (and any attachments) to standard out.

Then there is the `import` subcommand. You run this command on the receiving end and pipe the Kubernetes logs through it.

Finally, there is a specialized variant of `export` in the `exec` subcommand. You run this again in your pod mut use it to wrap the test launch command.

You will want to include the toolkit in your test image, e.g.:

If you are using the `export` you will change your entry point to run something like `juxr export -r  **/TEST-*.xml` or whatever the test report pattern you need.

As you will likely want to have the pod terminate with a success/failure exit code, a simpler option is to just change the command that runs your tests from `some_command arg arg` to `juxr exec -r **/TEST-*.xml -- some_command arg arg` as this will take care of launching the command and propagating the error code after exporting the test reports.

Have a look at the `juxr help export` and `juxr help exec` for details of the other export options such as secret redaction and renaming of tests / suites.

If you do not want to use the `juxr` toolkit inside your test container you can achieve the same result with base64 encoding, however this will not provide for automatic exporting of JUnit attachments or the ability to prefix/suffix the test names or suite.

```bash
needle="[[juxr::stream::$RANDOM::junit-test-report::TEST-custom.xml]]"
# NOTE: there must be a new line before the needle
echo "" 
echo $needle
base64 < TEST-custom.xml
echo $needle
```         

Then on the receiving end you just pipe the logs through `juxr import`, e.g. if using `helm test` to run your test container:

```
helm test --logs | juxr import -o helm-test-results/
```                                   

The import command will output all non-needle bookended content to standard out and write the files to the specified output directory

## Convert TAP formatted reports to JUnit XML format

> As a developer I have a testing tool that outputs TAP formatted test reports but I need to consume JUnit XML formatted reports

The `tap` subcommand will convert TAP version 12 or 13 output into JUnit XML format.

You can either pipe the output through or have the test command run by the toolkit, e.g.

```
some_command arg arg | juxr tap --name "some_command.tests" -o test-results/
```                                       

or 
```
juxr tap --name "some_command.tests" -o test-results/ -- some_command arg arg
```     

The later form will attempt to infer test durations and will propagate the exit code                                  

## Generate a JUnit XML report from executing a single command

> As a developer I have a single command which I would like to turn into a JUnit XML report

The `test` sub command will run a single command and produce a JUnit XML report that includes the commands output, e.g.

```
juxr test --name "some.command" --test "arg1 arg2" -o test-results/ -- some_command arg1 arg2
```            

This will produce a `TEST-some.command.xml` file in `test-results`.

Look at `juxr help test` for details on how to control the exit code mapping to differentiate the test status.

## Generate a JUnit XML report from running a suite of simple command

> As a developer, I have a series of different commands which represent independent tests I would like to run and record.

For this use case we recommend that you use something like the excellent [bats](https://github.com/bats-core/bats-core) test framework.
You can generate JUnit XML Foratted reports with this framework and you have control over the sequencing of tests.

However, sometimes you just need something quick and dirty... enter the `run` subcommand.

This takes a flexible YAML formatted description of a suite of tests and runs them in alphabetical order.
An example suite of tests could look like:

```yaml
"001 echo greeting using the current shell": >
  if [[ $(($(($RANDOM%10))%2)) -eq 1 ]] ;
  then
    echo hello world ;
  else
    echo hi world ;
  fi
"002 echo a greeting using the echo binary":
  - echo
  - "hello world"
"003 the false command when executed by the current shell should exit with non-zero":
  cmd: "false" # need to quote values that YAML might perform type conversions on
  success: 1
  failure: 0
"004 skip this test":
  cmd:
    - "false"
  skipped: 1
  failure: 2 # need to move it off it's default of 1
"005 exit code 1 or 2 is success 3 or 4 means skip and 5 or 6 is failure otherwise an error":
  cmd:
    - sh
    - "-c"
    - "exit 2"
  success:
    - 1
    - 2
  skipped:
    - 3
    - 4
  failure:
    - 5
    - 6
```                             

The top level keys are the test names.

* If the value of the key is a string then it will be executed using `sh -c` (or `cmd /C` on Windows)
* If the value of the key is an array of strings then the first value will be used as the program name and the remaining values will be passed as the program arguments
* If the value of the key is an object then the command will be taken from the `cmd` or `command` key in the object (which takes a string or an array of strings).
* The object form also permits specifying the expected exit codes for different test statuses

  * `success` takes either a single value or an array of values indicating the exit code of the program to interpret as a successful test result. If not specified then it is assumed to have the value `0`. 
  * `failure` takes either a single value or an array of values indicating the exit code of the program to interpret as a failed test result. If not specified then it is assumed to have the value `1`. 
  * `skipped` takes either a single value or an array of values indicating the exit code of the program to interpret as a successful test result. If not specified then the test will never be skipped.
  
  Note: if you want to use exit code `1` or `0` for anything other than `failure` or `success` respectively then you will need to override the defaults
  
  Note: an exit code other than those defined for `success`, `failure` or `skipped` will mark the test result as an error. 

