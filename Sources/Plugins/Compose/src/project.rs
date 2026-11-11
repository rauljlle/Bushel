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

use crate::backend::Backend;
use crate::health::HealthcheckSpec;
use crate::interpolate::interpolate;
use crate::model::{
    dependency_order, load_environment, scalar_to_string, Build, ComposeFile, DependencyCondition,
    Mount, Resource, Service,
};
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

pub struct Project {
    pub file: ComposeFile,
    pub name: String,
    pub compose_dir: PathBuf,
    pub environment: HashMap<String, String>,
    pub backend: Backend,
}

impl Project {
    pub fn load(
        file: Option<PathBuf>,
        project_name: Option<String>,
        env_files: &[PathBuf],
        backend: Backend,
    ) -> Result<Self> {
        let compose_path = find_compose_file(file)?;
        let compose_path = std::fs::canonicalize(&compose_path)
            .with_context(|| format!("failed to resolve {}", compose_path.display()))?;
        let compose_dir = compose_path
            .parent()
            .context("Compose file has no parent directory")?
            .to_path_buf();
        let environment = load_environment(&compose_dir, env_files)?;
        let contents = std::fs::read_to_string(&compose_path)
            .with_context(|| format!("failed to read {}", compose_path.display()))?;
        let expanded = interpolate(&contents, &environment)?;
        let file: ComposeFile = serde_yaml::from_str(&expanded)
            .with_context(|| format!("failed to parse {}", compose_path.display()))?;
        if file.services.is_empty() {
            bail!("Compose file defines no services");
        }
        let fallback_name = compose_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("default");
        let name = normalize_name(
            project_name
                .as_deref()
                .or(file.name.as_deref())
                .unwrap_or(fallback_name),
        )?;
        validate(&file, &name)?;
        Ok(Self {
            file,
            name,
            compose_dir,
            environment,
            backend,
        })
    }

    pub fn services(&self, selected: &[String]) -> Result<Vec<String>> {
        dependency_order(&self.file, selected)
    }

    pub fn selected_services(&self, selected: &[String]) -> Result<Vec<String>> {
        if selected.is_empty() {
            return Ok(self.file.services.keys().cloned().collect());
        }
        for service in selected {
            if !self.file.services.contains_key(service) {
                bail!("unknown service: {service}");
            }
        }
        Ok(selected.to_vec())
    }

    pub fn container_name(&self, service: &str) -> String {
        self.file.services[service]
            .container_name
            .clone()
            .unwrap_or_else(|| format!("{}-{service}-1", self.name))
    }

    pub fn owned_container_exists(&self, service: &str) -> Result<bool> {
        let name = self.container_name(service);
        let Some(output) = self.backend.output_quiet(["inspect", &name])? else {
            return Ok(false);
        };
        ensure_owned_container(&output, &name, &self.name, service)?;
        Ok(true)
    }

    fn running_service_ip(&self, service: &str, networks: &HashSet<&str>) -> Result<Option<String>> {
        let name = self.container_name(service);
        let Some(output) = self.backend.output_quiet(["inspect", &name])? else {
            return Ok(None);
        };
        ensure_owned_container(&output, &name, &self.name, service)?;
        let containers: Vec<serde_yaml::Value> = serde_yaml::from_slice(&output)?;
        let attachments = containers
            .first()
            .and_then(|container| container.get("status"))
            .and_then(|status| status.get("networks"))
            .and_then(serde_yaml::Value::as_sequence);
        Ok(attachments.and_then(|items| {
            items.iter().find_map(|item| {
                let network = item.get("network")?.as_str()?;
                if !networks.contains(network) {
                    return None;
                }
                let address = item.get("ipv4Address")?.as_str()?.split('/').next()?;
                address.parse::<std::net::Ipv4Addr>().ok().map(|_| address.to_owned())
            })
        }))
    }

    pub fn probe_health(&self, service: &str) -> Result<Option<bool>> {
        let Some(check) = self.healthcheck(service)? else {
            return Ok(None);
        };
        Ok(Some(self.run_healthcheck(service, &check)?))
    }

    pub fn wait_healthy(&self, service: &str) -> Result<()> {
        let check = self
            .healthcheck(service)?
            .with_context(|| format!("service {service} has no active healthcheck"))?;
        eprintln!("Waiting for {service} to become healthy...");
        let started = Instant::now();
        let mut failures = 0;
        loop {
            if self.run_healthcheck(service, &check)? {
                return Ok(());
            }
            if started.elapsed() >= check.start_period {
                failures += 1;
                if failures >= check.retries {
                    bail!(
                        "service {service} did not become healthy after {} attempts",
                        failures
                    );
                }
            }
            let interval = if started.elapsed() < check.start_period {
                check.start_interval
            } else {
                check.interval
            };
            thread::sleep(interval);
        }
    }

    fn healthcheck(&self, service: &str) -> Result<Option<HealthcheckSpec>> {
        self.file.services[service]
            .healthcheck
            .as_ref()
            .map(HealthcheckSpec::parse)
            .transpose()
            .map(Option::flatten)
            .with_context(|| format!("invalid healthcheck for service {service}"))
    }

    fn run_healthcheck(&self, service: &str, check: &HealthcheckSpec) -> Result<bool> {
        let mut command = vec!["exec".to_owned(), self.container_name(service)];
        command.extend(check.command.iter().cloned());
        self.backend.run_quiet_timeout(command, check.timeout)
    }

    pub fn default_network(&self) -> String {
        format!("{}-default", self.name)
    }

    pub fn network_name(&self, name: &str) -> Result<String> {
        if name == "default" && !self.file.networks.contains_key(name) {
            return Ok(self.default_network());
        }
        let resource = self
            .file
            .networks
            .get(name)
            .with_context(|| format!("service references undefined network {name}"))?;
        Ok(resource_name(&self.name, name, resource))
    }

    pub fn volume_name(&self, name: &str) -> Result<String> {
        let resource = self
            .file
            .volumes
            .get(name)
            .with_context(|| format!("service references undefined volume {name}"))?;
        Ok(resource_name(&self.name, name, resource))
    }

    pub fn ensure_resources(&self, services: &[String]) -> Result<()> {
        let mut networks = IndexMap::<String, ()>::new();
        for service_name in services {
            let service = &self.file.services[service_name];
            if service.networks.0.is_empty() {
                networks.insert(self.default_network(), ());
            } else {
                for network in &service.networks.0 {
                    networks.insert(self.network_name(network)?, ());
                }
            }
        }
        for network in networks.keys() {
            let external = self
                .file
                .networks
                .iter()
                .find(|(name, _)| self.network_name(name).ok().as_ref() == Some(network))
                .is_some_and(|(_, resource)| resource.external);
            if !self.backend.run_quiet(["network", "inspect", network])? {
                if external {
                    bail!("external network {network} does not exist");
                }
                self.backend.run([
                    "network",
                    "create",
                    "--label",
                    &format!("com.docker.compose.project={}", self.name),
                    network,
                ])?;
            }
        }

        for (volume, resource) in &self.file.volumes {
            let name = resource_name(&self.name, volume, resource);
            if !self.backend.run_quiet(["volume", "inspect", &name])? {
                if resource.external {
                    bail!("external volume {name} does not exist");
                }
                self.backend.run([
                    "volume",
                    "create",
                    "--label",
                    &format!("com.docker.compose.project={}", self.name),
                    &name,
                ])?;
            }
        }
        Ok(())
    }

    pub fn image_for(&self, service_name: &str, service: &Service) -> String {
        service.image.clone().unwrap_or_else(|| {
            format!(
                "localhost/{}/{}:latest",
                self.name,
                normalize_fragment(service_name)
            )
        })
    }

    pub fn build_service(&self, service_name: &str, no_cache: bool) -> Result<()> {
        let Some(command) = self.build_command(service_name, no_cache)? else {
            return Ok(());
        };
        self.backend.run(command)
    }

    fn build_command(&self, service_name: &str, no_cache: bool) -> Result<Option<Vec<OsString>>> {
        let service = &self.file.services[service_name];
        let Some(build) = &service.build else {
            return Ok(None);
        };
        let (context, dockerfile, args, target, build_platforms) = match build {
            Build::Context(context) => (context.as_str(), None, None, None, &[][..]),
            Build::Detailed(options) => (
                options.context.as_str(),
                options.dockerfile.as_deref(),
                Some(&options.args),
                options.target.as_deref(),
                options.platforms.as_slice(),
            ),
        };
        let context = absolute(&self.compose_dir, context);
        let mut command = vec![OsString::from("build")];
        command.extend([
            OsString::from("--tag"),
            OsString::from(self.image_for(service_name, service)),
        ]);
        if no_cache {
            command.push(OsString::from("--no-cache"));
        }
        if let Some(dockerfile) = dockerfile {
            command.extend([
                OsString::from("--file"),
                absolute(&self.compose_dir, dockerfile).into_os_string(),
            ]);
        }
        if let Some(args) = args {
            for (key, value) in &args.0 {
                command.push(OsString::from("--build-arg"));
                command.push(OsString::from(match value {
                    Some(value) => format!("{key}={value}"),
                    None => key.clone(),
                }));
            }
        }
        if let Some(target) = target {
            command.extend([OsString::from("--target"), OsString::from(target)]);
        }
        if let Some(platform) = &service.platform {
            if !build_platforms.is_empty() && !build_platforms.contains(platform) {
                bail!("service {service_name}: platform {platform} is not in build.platforms");
            }
        }
        if build_platforms.is_empty() {
            if let Some(platform) = &service.platform {
                push_option(&mut command, "--platform", platform);
            }
        } else {
            for platform in build_platforms {
                push_option(&mut command, "--platform", platform);
            }
        }
        command.push(context.into_os_string());
        Ok(Some(command))
    }

    pub fn run_service(&self, service_name: &str) -> Result<()> {
        let service = &self.file.services[service_name];
        let container_name = self.container_name(service_name);
        if self.owned_container_exists(service_name)? {
            self.backend.run(["delete", "--force", &container_name])?;
        }

        let mut command = vec![
            OsString::from("run"),
            OsString::from("--detach"),
            OsString::from("--name"),
            OsString::from(&container_name),
        ];
        for label in [
            format!("com.docker.compose.project={}", self.name),
            format!("com.docker.compose.service={service_name}"),
            "com.docker.compose.container-number=1".to_owned(),
        ] {
            push_option(&mut command, "--label", label);
        }
        for (key, value) in &service.labels.0 {
            push_option(
                &mut command,
                "--label",
                value
                    .as_ref()
                    .map(|value| format!("{key}={value}"))
                    .unwrap_or_else(|| key.clone()),
            );
        }
        for env_file in &service.env_file.0 {
            push_option(
                &mut command,
                "--env-file",
                absolute(&self.compose_dir, env_file).into_os_string(),
            );
        }
        for (key, value) in &service.environment.0 {
            let value = value.clone().or_else(|| self.environment.get(key).cloned());
            push_option(
                &mut command,
                "--env",
                value
                    .map(|value| format!("{key}={value}"))
                    .unwrap_or_else(|| key.clone()),
            );
        }
        for port in &service.ports {
            push_option(&mut command, "--publish", scalar_to_string(port)?);
        }
        for mount in &service.volumes {
            push_option(&mut command, "--volume", self.mount_value(mount)?);
        }
        let networks = if service.networks.0.is_empty() {
            vec![self.default_network()]
        } else {
            service
                .networks
                .0
                .iter()
                .map(|name| self.network_name(name))
                .collect::<Result<Vec<_>>>()?
        };
        for network in &networks {
            push_option(&mut command, "--network", format!("{network},hostname={service_name}"));
        }
        if !self.backend.is_dry_run() {
            let active_networks: HashSet<&str> = networks.iter().map(String::as_str).collect();
            for (peer_name, peer) in &self.file.services {
                if peer_name == service_name {
                    continue;
                }
                let peer_networks = if peer.networks.0.is_empty() {
                    vec![self.default_network()]
                } else {
                    peer.networks
                        .0
                        .iter()
                        .map(|name| self.network_name(name))
                        .collect::<Result<Vec<_>>>()?
                };
                if !peer_networks.iter().any(|name| active_networks.contains(name.as_str())) {
                    continue;
                }
                if let Some(ip) = self.running_service_ip(peer_name, &active_networks)? {
                    push_option(&mut command, "--add-host", format!("{peer_name}:{ip}"));
                }
            }
        }
        for capability in &service.cap_add {
            push_option(&mut command, "--cap-add", capability);
        }
        for capability in &service.cap_drop {
            push_option(&mut command, "--cap-drop", capability);
        }
        if let Some(value) = &service.working_dir {
            push_option(&mut command, "--workdir", value);
        }
        if let Some(value) = &service.user {
            push_option(&mut command, "--user", value);
        }
        if let Some(value) = &service.platform {
            push_option(&mut command, "--platform", value);
        }
        if let Some(value) = &service.cpus {
            push_option(&mut command, "--cpus", scalar_to_string(value)?);
        }
        if let Some(value) = &service.mem_limit {
            push_option(&mut command, "--memory", value);
        }
        if let Some(value) = &service.shm_size {
            push_option(&mut command, "--shm-size", value);
        }
        for value in &service.tmpfs.0 {
            push_option(&mut command, "--tmpfs", value);
        }
        if service.read_only.unwrap_or(false) {
            command.push(OsString::from("--read-only"));
        }
        if service.init.unwrap_or(false) {
            command.push(OsString::from("--init"));
        }
        if service.stdin_open.unwrap_or(false) {
            command.push(OsString::from("--interactive"));
        }
        if service.tty.unwrap_or(false) {
            command.push(OsString::from("--tty"));
        }
        if let Some(entrypoint) = &service.entrypoint {
            let parts = entrypoint.arguments()?;
            if parts.len() != 1 {
                bail!("service {service_name}: list-valued entrypoint is not supported by the Bushel CLI");
            }
            push_option(&mut command, "--entrypoint", &parts[0]);
        }
        command.push(OsString::from(self.image_for(service_name, service)));
        if let Some(arguments) = &service.command {
            command.extend(arguments.arguments()?.into_iter().map(OsString::from));
        }
        self.backend.run(command)
    }

    pub fn stop_and_delete(&self, service_name: &str) -> Result<()> {
        let name = self.container_name(service_name);
        if !self.owned_container_exists(service_name)? {
            return Ok(());
        }
        let _ = self.backend.run_quiet(["stop", &name])?;
        self.backend.run(["delete", "--force", &name])
    }

    pub fn remove_resources(&self, volumes: bool) -> Result<()> {
        let mut networks = vec![self.default_network()];
        for (name, resource) in &self.file.networks {
            if !resource.external {
                networks.push(resource_name(&self.name, name, resource));
            }
        }
        networks.sort();
        networks.dedup();
        for network in networks {
            if self.backend.run_quiet(["network", "inspect", &network])? {
                self.backend.run(["network", "delete", &network])?;
            }
        }
        if volumes {
            for (name, resource) in &self.file.volumes {
                if resource.external {
                    continue;
                }
                let name = resource_name(&self.name, name, resource);
                if self.backend.run_quiet(["volume", "inspect", &name])? {
                    self.backend.run(["volume", "delete", &name])?;
                }
            }
        }
        Ok(())
    }

    fn mount_value(&self, mount: &Mount) -> Result<OsString> {
        match mount {
            Mount::Short(value) => {
                let mut parts = value.splitn(3, ':');
                let source = parts.next().unwrap_or_default();
                let Some(target) = parts.next() else {
                    return Ok(OsString::from(value));
                };
                let options = parts.next();
                let source = self.mount_source(source)?;
                let mut rendered = format!("{}:{target}", source.to_string_lossy());
                if let Some(options) = options {
                    rendered.push(':');
                    rendered.push_str(options);
                }
                Ok(OsString::from(rendered))
            }
            Mount::Long(value) => {
                let mut rendered = match value.kind.as_str() {
                    "bind" => {
                        let source = value
                            .source
                            .as_deref()
                            .context("bind mount requires source")?;
                        let source = absolute(&self.compose_dir, source);
                        if !self.backend.is_dry_run()
                            && value
                                .bind
                                .as_ref()
                                .map_or(true, |bind| bind.create_host_path)
                        {
                            ensure_bind_source(&source)?;
                        }
                        format!("{}:{}", source.display(), value.target)
                    }
                    "volume" => {
                        let source = value
                            .source
                            .as_deref()
                            .context("volume mount requires source")?;
                        format!("{}:{}", self.volume_name(source)?, value.target)
                    }
                    other => bail!("unsupported mount type {other}"),
                };
                if value.read_only {
                    rendered.push_str(":ro");
                }
                Ok(OsString::from(rendered))
            }
        }
    }

    fn mount_source(&self, source: &str) -> Result<OsString> {
        if source.starts_with('.') || source.starts_with('/') || source.starts_with('~') {
            let source = absolute(&self.compose_dir, source);
            if !self.backend.is_dry_run() {
                ensure_bind_source(&source)?;
            }
            return Ok(source.into_os_string());
        }
        Ok(OsString::from(self.volume_name(source)?))
    }
}

fn ensure_bind_source(source: &Path) -> Result<()> {
    if !source.exists() {
        std::fs::create_dir_all(source)
            .with_context(|| format!("failed to create bind mount source {}", source.display()))?;
    }
    Ok(())
}

fn validate(file: &ComposeFile, project_name: &str) -> Result<()> {
    let mut container_names = HashSet::new();
    for (name, service) in &file.services {
        if service.image.is_none() && service.build.is_none() {
            bail!("service {name} must define image or build");
        }
        let container_name = service
            .container_name
            .clone()
            .unwrap_or_else(|| format!("{project_name}-{name}-1"));
        if service.container_name.is_some() && !valid_container_name(&container_name) {
            bail!("service {name}: invalid container_name '{container_name}'");
        }
        if !container_names.insert(container_name.clone()) {
            bail!("duplicate container_name '{container_name}'");
        }
        if let Some(healthcheck) = &service.healthcheck {
            HealthcheckSpec::parse(healthcheck)
                .with_context(|| format!("invalid healthcheck for service {name}"))?;
        }
        for (dependency, options) in &service.depends_on.0 {
            if matches!(options.condition, DependencyCondition::ServiceHealthy) {
                let dependency_service = file.services.get(dependency).with_context(|| {
                    format!("service {name} references undefined dependency {dependency}")
                })?;
                let has_check = dependency_service
                    .healthcheck
                    .as_ref()
                    .map(HealthcheckSpec::parse)
                    .transpose()?
                    .flatten()
                    .is_some();
                if !has_check {
                    bail!("service {name} requires {dependency} to be healthy, but {dependency} has no active healthcheck");
                }
            }
        }
        for key in service.extensions.keys() {
            if !key.starts_with("x-") {
                eprintln!("warning: service {name}: unsupported Compose field '{key}' is ignored");
            }
        }
    }
    dependency_order(file, &[])?;
    Ok(())
}

fn valid_container_name(name: &str) -> bool {
    name.len() <= 63
        && name.len() >= 2
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn ensure_owned_container(output: &[u8], name: &str, project: &str, service: &str) -> Result<()> {
    let containers: Vec<serde_yaml::Value> = serde_yaml::from_slice(output)
        .with_context(|| format!("failed to parse inspect output for container {name}"))?;
    let labels = containers
        .first()
        .context("inspect returned no containers")?
        .get("configuration")
        .and_then(|configuration| configuration.get("labels"));
    let matches_project = labels
        .and_then(|labels| labels.get("com.docker.compose.project"))
        .and_then(serde_yaml::Value::as_str)
        == Some(project);
    let matches_service = labels
        .and_then(|labels| labels.get("com.docker.compose.service"))
        .and_then(serde_yaml::Value::as_str)
        == Some(service);
    if !matches_project || !matches_service {
        bail!("container {name} already exists but is not owned by service {project}/{service}");
    }
    Ok(())
}

fn find_compose_file(requested: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = requested {
        if path.exists() {
            return Ok(path);
        }
        bail!("Compose file does not exist: {}", path.display());
    }
    for candidate in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!("no Compose file found; use --file to specify one")
}

fn normalize_name(value: &str) -> Result<String> {
    let output = normalize_fragment(value);
    if output.is_empty() {
        bail!("project name contains no valid characters");
    }
    Ok(output)
}

fn normalize_fragment(value: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for character in value.to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character);
            previous_separator = false;
        } else if !previous_separator && !output.is_empty() {
            output.push('-');
            previous_separator = true;
        }
    }
    output.trim_matches('-').to_owned()
}

fn resource_name(project: &str, logical_name: &str, resource: &Resource) -> String {
    resource
        .name
        .clone()
        .unwrap_or_else(|| format!("{project}-{logical_name}"))
}

fn absolute(base: &Path, value: &str) -> PathBuf {
    if value == "~" || value.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let suffix = value.strip_prefix("~/").unwrap_or_default();
            return PathBuf::from(home).join(suffix);
        }
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn push_option<T: Into<OsString>>(command: &mut Vec<OsString>, name: &str, value: T) {
    command.push(OsString::from(name));
    command.push(value.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TemporaryComposeDir(PathBuf);

    impl TemporaryComposeDir {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "bushel-compose-bind-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TemporaryComposeDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn project_in(directory: &Path, dry_run: bool) -> Project {
        Project {
            file: ComposeFile::default(),
            name: "example".to_owned(),
            compose_dir: directory.to_path_buf(),
            environment: HashMap::new(),
            backend: Backend::from_environment(dry_run, false),
        }
    }

    #[test]
    fn short_bind_mount_creates_missing_directory_and_preserves_existing_files() {
        let temporary = TemporaryComposeDir::new();
        let project = project_in(&temporary.0, false);
        let mount = Mount::Short("./init-scripts:/docker-entrypoint-initdb.d".to_owned());
        project.mount_value(&mount).unwrap();
        assert!(temporary.0.join("init-scripts").is_dir());

        let source_file = temporary.0.join("config.txt");
        std::fs::write(&source_file, "original").unwrap();
        project
            .mount_value(&Mount::Short("./config.txt:/etc/config.txt".to_owned()))
            .unwrap();
        assert_eq!(std::fs::read_to_string(source_file).unwrap(), "original");
    }

    #[test]
    fn long_bind_mount_honors_create_host_path() {
        let temporary = TemporaryComposeDir::new();
        let project = project_in(&temporary.0, false);
        let default_mount: Mount =
            serde_yaml::from_str("type: bind\nsource: ./created\ntarget: /data\n").unwrap();
        project.mount_value(&default_mount).unwrap();
        assert!(temporary.0.join("created").is_dir());

        let disabled_mount: Mount = serde_yaml::from_str(
            "type: bind\nsource: ./not-created\ntarget: /data\nbind:\n  create_host_path: false\n",
        )
        .unwrap();
        project.mount_value(&disabled_mount).unwrap();
        assert!(!temporary.0.join("not-created").exists());
    }

    #[test]
    fn dry_run_and_named_volumes_do_not_create_host_directories() {
        let temporary = TemporaryComposeDir::new();
        let project = project_in(&temporary.0, true);
        project
            .mount_value(&Mount::Short("./not-created:/data".to_owned()))
            .unwrap();
        assert!(!temporary.0.join("not-created").exists());

        let mut project = project_in(&temporary.0, false);
        project
            .file
            .volumes
            .insert("cache".to_owned(), Resource::default());
        assert_eq!(
            project
                .mount_value(&Mount::Short("cache:/cache".to_owned()))
                .unwrap(),
            OsString::from("example-cache:/cache")
        );
        assert!(!temporary.0.join("cache").exists());
    }

    #[test]
    fn uses_explicit_container_names_for_all_service_operations() {
        let file: ComposeFile = serde_yaml::from_str(
            "services:\n  postgres:\n    image: postgres:17\n    container_name: duop_postgres\n  worker:\n    image: worker:latest\n",
        )
        .unwrap();
        assert!(!file.services["postgres"]
            .extensions
            .contains_key("container_name"));
        validate(&file, "duop").unwrap();
        let project = Project {
            file,
            name: "duop".to_owned(),
            compose_dir: PathBuf::new(),
            environment: HashMap::new(),
            backend: Backend::from_environment(true, false),
        };
        assert_eq!(project.container_name("postgres"), "duop_postgres");
        assert_eq!(project.container_name("worker"), "duop-worker-1");
    }

    #[test]
    fn rejects_duplicate_or_invalid_container_names() {
        let duplicate: ComposeFile = serde_yaml::from_str(
            "services:\n  first:\n    image: alpine\n    container_name: shared\n  second:\n    image: alpine\n    container_name: shared\n",
        )
        .unwrap();
        assert!(validate(&duplicate, "project").is_err());

        let invalid: ComposeFile = serde_yaml::from_str(
            "services:\n  first:\n    image: alpine\n    container_name: 'bad name'\n",
        )
        .unwrap();
        assert!(validate(&invalid, "project").is_err());
    }

    #[test]
    fn refuses_to_recreate_a_container_owned_by_another_service() {
        let owned = br#"[{"configuration":{"labels":{"com.docker.compose.project":"duop","com.docker.compose.service":"postgres"}}}]"#;
        assert!(ensure_owned_container(owned, "duop_postgres", "duop", "postgres").is_ok());
        assert!(ensure_owned_container(owned, "duop_postgres", "other", "postgres").is_err());
        assert!(ensure_owned_container(owned, "duop_postgres", "duop", "worker").is_err());
    }

    #[test]
    fn healthy_dependency_requires_an_active_healthcheck() {
        let missing: ComposeFile = serde_yaml::from_str(
            "services:\n  db:\n    image: postgres\n  worker:\n    image: worker\n    depends_on:\n      db:\n        condition: service_healthy\n",
        )
        .unwrap();
        assert!(validate(&missing, "duop").is_err());
        let present: ComposeFile = serde_yaml::from_str(
            "services:\n  db:\n    image: postgres\n    healthcheck:\n      test: [CMD, pg_isready]\n  worker:\n    image: worker\n    depends_on:\n      db:\n        condition: service_healthy\n",
        )
        .unwrap();
        validate(&present, "duop").unwrap();
    }

    #[test]
    fn waits_for_healthy_probe_and_fails_after_retries() {
        let file: ComposeFile = serde_yaml::from_str(
            "services:\n  db:\n    image: postgres\n    healthcheck:\n      test: [CMD, pg_isready]\n      interval: 1ms\n      timeout: 100ms\n      retries: 2\n",
        )
        .unwrap();
        let make_project = |backend| Project {
            file: file.clone(),
            name: "duop".to_owned(),
            compose_dir: PathBuf::new(),
            environment: HashMap::new(),
            backend,
        };
        make_project(Backend::for_test("/usr/bin/true"))
            .wait_healthy("db")
            .unwrap();
        assert!(make_project(Backend::for_test("/usr/bin/false"))
            .wait_healthy("db")
            .is_err());
    }

    #[test]
    fn forwards_service_platform_to_build() {
        let file: ComposeFile = serde_yaml::from_str(
            "services:\n  worker:\n    build:\n      context: .\n      target: development\n    platform: linux/amd64\n",
        )
        .unwrap();
        let project = Project {
            file,
            name: "duop".to_owned(),
            compose_dir: PathBuf::from("/tmp/duop"),
            environment: HashMap::new(),
            backend: Backend::from_environment(true, false),
        };
        let command = project.build_command("worker", false).unwrap().unwrap();
        let command = command
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(command
            .windows(2)
            .any(|pair| pair == ["--platform", "linux/amd64"]));
        assert!(command
            .windows(2)
            .any(|pair| pair == ["--target", "development"]));
    }

    #[test]
    fn accepts_explicit_build_platforms_and_rejects_mismatch() {
        let file: ComposeFile = serde_yaml::from_str(
            "services:\n  worker:\n    build:\n      context: .\n      platforms: [linux/amd64, linux/arm64]\n    platform: linux/amd64\n",
        )
        .unwrap();
        let project = Project {
            file,
            name: "duop".to_owned(),
            compose_dir: PathBuf::from("/tmp/duop"),
            environment: HashMap::new(),
            backend: Backend::from_environment(true, false),
        };
        let command = project.build_command("worker", false).unwrap().unwrap();
        let command = command
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>();
        assert_eq!(
            command
                .windows(2)
                .filter(|pair| pair[0] == "--platform")
                .count(),
            2
        );

        let mut mismatch = project;
        mismatch.file.services.get_mut("worker").unwrap().platform = Some("linux/s390x".to_owned());
        assert!(mismatch.build_command("worker", false).is_err());
    }
}
