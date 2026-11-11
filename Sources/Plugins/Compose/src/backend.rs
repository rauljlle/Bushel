// Copyright © 2026 Apple Inc. and the container project authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Backend {
    program: OsString,
    dry_run: bool,
    verbose: bool,
}

impl Backend {
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    #[cfg(test)]
    pub fn for_test(program: &str) -> Self {
        Self {
            program: OsString::from(program),
            dry_run: false,
            verbose: false,
        }
    }

    pub fn from_environment(dry_run: bool, verbose: bool) -> Self {
        let program = std::env::var_os("BUSHEL_COMPOSE_BACKEND")
            .or_else(|| std::env::var_os("CONTAINER_COMPOSE_BACKEND"))
            .unwrap_or_else(|| OsString::from("bushel"));
        Self {
            program,
            dry_run,
            verbose,
        }
    }

    pub fn run<I, S>(&self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        self.print_command(&args);
        if self.dry_run {
            return Ok(());
        }
        let status = Command::new(&self.program)
            .args(&args)
            .status()
            .with_context(|| format!("failed to execute {}", self.program.to_string_lossy()))?;
        self.require_success(status, &args)
    }

    pub fn run_quiet<I, S>(&self, args: I) -> Result<bool>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        self.print_command(&args);
        if self.dry_run {
            return Ok(false);
        }
        let status = Command::new(&self.program)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("failed to execute {}", self.program.to_string_lossy()))?;
        Ok(status.success())
    }

    pub fn output_quiet<I, S>(&self, args: I) -> Result<Option<Vec<u8>>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        self.print_command(&args);
        if self.dry_run {
            return Ok(None);
        }
        let output = Command::new(&self.program)
            .args(&args)
            .stderr(Stdio::null())
            .output()
            .with_context(|| format!("failed to execute {}", self.program.to_string_lossy()))?;
        Ok(output.status.success().then_some(output.stdout))
    }

    pub fn run_quiet_timeout<I, S>(&self, args: I, timeout: Duration) -> Result<bool>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        self.print_command(&args);
        if self.dry_run {
            return Ok(true);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .context("healthcheck timeout exceeds the supported range")?;
        let mut child = Command::new(&self.program)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to execute {}", self.program.to_string_lossy()))?;
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status.success());
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn spawn<I, S>(&self, args: I) -> Result<std::process::Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        self.print_command(&args);
        if self.dry_run {
            return Command::new("true")
                .spawn()
                .context("failed to run dry-run stub");
        }
        Command::new(&self.program)
            .args(&args)
            .spawn()
            .with_context(|| format!("failed to execute {}", self.program.to_string_lossy()))
    }

    fn require_success(&self, status: ExitStatus, args: &[OsString]) -> Result<()> {
        if status.success() {
            return Ok(());
        }
        let rendered = render_command(&self.program, args);
        match status.code() {
            Some(code) => bail!("command failed with exit code {code}: {rendered}"),
            None => bail!("command terminated by signal: {rendered}"),
        }
    }

    fn print_command(&self, args: &[OsString]) {
        if self.verbose || self.dry_run {
            eprintln!("$ {}", render_command(&self.program, args));
        }
    }
}

fn render_command(program: &OsStr, args: &[OsString]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(OsString::as_os_str))
        .map(|part| shell_words::quote(&part.to_string_lossy()).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
