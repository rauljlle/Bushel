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
use crate::interpolate::interpolate;
use crate::model::{
    dependency_order, load_environment, scalar_to_string, Build, ComposeFile, Mount, Resource,
    Service,
};
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

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
        validate(&file)?;
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
        format!("{}-{service}-1", self.name)
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
        let service = &self.file.services[service_name];
        let Some(build) = &service.build else {
            return Ok(());
        };
        let (context, dockerfile, args, target) = match build {
            Build::Context(context) => (context.as_str(), None, None, None),
            Build::Detailed(options) => (
                options.context.as_str(),
                options.dockerfile.as_deref(),
                Some(&options.args),
                options.target.as_deref(),
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
        command.push(context.into_os_string());
        self.backend.run(command)
    }

    pub fn run_service(&self, service_name: &str) -> Result<()> {
        let service = &self.file.services[service_name];
        let container_name = self.container_name(service_name);
        if self.backend.run_quiet(["inspect", &container_name])? {
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
        for network in networks {
            push_option(&mut command, "--network", network);
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
        if !self.backend.run_quiet(["inspect", &name])? {
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
                        format!(
                            "{}:{}",
                            absolute(&self.compose_dir, source).display(),
                            value.target
                        )
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
            return Ok(absolute(&self.compose_dir, source).into_os_string());
        }
        Ok(OsString::from(self.volume_name(source)?))
    }
}

fn validate(file: &ComposeFile) -> Result<()> {
    for (name, service) in &file.services {
        if service.image.is_none() && service.build.is_none() {
            bail!("service {name} must define image or build");
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
