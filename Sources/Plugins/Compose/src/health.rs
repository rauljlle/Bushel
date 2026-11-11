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

use crate::model::Healthcheck;
use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::time::Duration;

pub struct HealthcheckSpec {
    pub command: Vec<String>,
    pub interval: Duration,
    pub timeout: Duration,
    pub retries: u32,
    pub start_period: Duration,
    pub start_interval: Duration,
}

impl HealthcheckSpec {
    pub fn parse(input: &Healthcheck) -> Result<Option<Self>> {
        let command = match (&input.test, input.disable) {
            (_, true) | (None, _) => return Ok(None),
            (Some(Value::String(test)), _) => vec!["/bin/sh".into(), "-c".into(), test.clone()],
            (Some(Value::Sequence(parts)), _) => {
                let parts = parts
                    .iter()
                    .map(|part| {
                        part.as_str()
                            .map(str::to_owned)
                            .context("healthcheck.test must contain strings")
                    })
                    .collect::<Result<Vec<_>>>()?;
                match parts.first().map(String::as_str) {
                    Some("NONE") if parts.len() == 1 => return Ok(None),
                    Some("CMD") if parts.len() > 1 => parts[1..].to_vec(),
                    Some("CMD-SHELL") if parts.len() == 2 => {
                        vec!["/bin/sh".into(), "-c".into(), parts[1].clone()]
                    }
                    _ => bail!("healthcheck.test must be CMD, CMD-SHELL, or NONE"),
                }
            }
            _ => bail!("healthcheck.test must be a string or list"),
        };
        let interval = parse_option(&input.interval, Duration::from_secs(30))?;
        let timeout = parse_option(&input.timeout, Duration::from_secs(30))?;
        let start_period = parse_option(&input.start_period, Duration::ZERO)?;
        let start_interval = parse_option(&input.start_interval, interval)?;
        let retries = input.retries.unwrap_or(3);
        if interval.is_zero() || timeout.is_zero() || start_interval.is_zero() || retries == 0 {
            bail!("healthcheck interval, timeout, start_interval, and retries must be positive");
        }
        Ok(Some(Self {
            command,
            interval,
            timeout,
            retries,
            start_period,
            start_interval,
        }))
    }
}

fn parse_option(value: &Option<String>, default: Duration) -> Result<Duration> {
    value
        .as_deref()
        .map(parse_duration)
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn parse_duration(value: &str) -> Result<Duration> {
    let value = value.trim();
    for (suffix, multiplier) in [
        ("ns", 0.000_000_001),
        ("us", 0.000_001),
        ("ms", 0.001),
        ("s", 1.0),
        ("m", 60.0),
        ("h", 3600.0),
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            let amount: f64 = number
                .parse()
                .with_context(|| format!("invalid healthcheck duration '{value}'"))?;
            let seconds = amount * multiplier;
            if !seconds.is_finite() || seconds < 0.0 || seconds >= (u64::MAX / 2) as f64 {
                bail!("invalid healthcheck duration '{value}'");
            }
            return Ok(Duration::from_secs_f64(seconds));
        }
    }
    bail!("invalid healthcheck duration '{value}'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compose_healthcheck_commands() {
        let shell: Healthcheck = serde_yaml::from_str(
            "test: [CMD-SHELL, 'pg_isready -U app']\ninterval: 10s\ntimeout: 5s\nretries: 5\n",
        )
        .unwrap();
        let shell = HealthcheckSpec::parse(&shell).unwrap().unwrap();
        assert_eq!(shell.command, ["/bin/sh", "-c", "pg_isready -U app"]);
        assert_eq!(shell.interval, Duration::from_secs(10));
        assert_eq!(shell.timeout, Duration::from_secs(5));
        assert_eq!(shell.retries, 5);

        let direct: Healthcheck =
            serde_yaml::from_str("test: [CMD, curl, -f, http://localhost]").unwrap();
        assert_eq!(
            HealthcheckSpec::parse(&direct).unwrap().unwrap().command,
            ["curl", "-f", "http://localhost"]
        );
    }

    #[test]
    fn rejects_invalid_or_disabled_checks() {
        let disabled: Healthcheck = serde_yaml::from_str("test: [NONE]").unwrap();
        assert!(HealthcheckSpec::parse(&disabled).unwrap().is_none());
        let invalid: Healthcheck = serde_yaml::from_str("test: [CMD, true]\ninterval: 0s").unwrap();
        assert!(HealthcheckSpec::parse(&invalid).is_err());
    }
}
