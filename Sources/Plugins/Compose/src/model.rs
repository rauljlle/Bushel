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
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ComposeFile {
    pub name: Option<String>,
    pub services: IndexMap<String, Service>,
    #[serde(default)]
    pub volumes: IndexMap<String, Resource>,
    #[serde(default)]
    pub networks: IndexMap<String, Resource>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Service {
    pub image: Option<String>,
    pub build: Option<Build>,
    pub command: Option<CommandValue>,
    pub entrypoint: Option<CommandValue>,
    #[serde(default)]
    pub environment: StringMap,
    #[serde(default)]
    pub env_file: StringList,
    #[serde(default)]
    pub ports: Vec<Value>,
    #[serde(default)]
    pub volumes: Vec<Mount>,
    #[serde(default)]
    pub networks: NameSet,
    #[serde(default)]
    pub depends_on: DependsOn,
    pub working_dir: Option<String>,
    pub user: Option<String>,
    pub platform: Option<String>,
    pub cpus: Option<Value>,
    pub mem_limit: Option<String>,
    #[serde(default)]
    pub labels: StringMap,
    #[serde(default)]
    pub cap_add: Vec<String>,
    #[serde(default)]
    pub cap_drop: Vec<String>,
    pub read_only: Option<bool>,
    pub shm_size: Option<String>,
    #[serde(default)]
    pub tmpfs: StringList,
    pub init: Option<bool>,
    pub stdin_open: Option<bool>,
    pub tty: Option<bool>,
    #[serde(flatten)]
    pub extensions: IndexMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Build {
    Context(String),
    Detailed(BuildOptions),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct BuildOptions {
    #[serde(default = "default_context")]
    pub context: String,
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub args: StringMap,
    pub target: Option<String>,
}

fn default_context() -> String {
    ".".to_owned()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CommandValue {
    String(String),
    List(Vec<String>),
}

impl CommandValue {
    pub fn arguments(&self) -> Result<Vec<String>> {
        match self {
            Self::String(value) => shell_words::split(value).context("invalid command quoting"),
            Self::List(values) => Ok(values.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Mount {
    Short(String),
    Long(LongMount),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LongMount {
    #[serde(rename = "type", default = "default_mount_type")]
    pub kind: String,
    pub source: Option<String>,
    pub target: String,
    #[serde(default)]
    pub read_only: bool,
}

fn default_mount_type() -> String {
    "volume".to_owned()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Resource {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_external")]
    pub external: bool,
}

fn deserialize_external<'de, D>(deserializer: D) -> std::result::Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        None => false,
        Some(Value::Bool(value)) => value,
        Some(Value::Mapping(_)) => true,
        Some(_) => {
            return Err(serde::de::Error::custom(
                "external must be a boolean or mapping",
            ))
        }
    })
}

#[derive(Clone, Debug, Default)]
pub struct StringMap(pub IndexMap<String, Option<String>>);

impl<'de> Deserialize<'de> for StringMap {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let mut output = IndexMap::new();
        match value {
            Value::Null => {}
            Value::Mapping(entries) => {
                for (key, value) in entries {
                    let key = scalar_to_string(&key).map_err(serde::de::Error::custom)?;
                    let value = match value {
                        Value::Null => None,
                        value => Some(scalar_to_string(&value).map_err(serde::de::Error::custom)?),
                    };
                    output.insert(key, value);
                }
            }
            Value::Sequence(entries) => {
                for entry in entries {
                    let entry = scalar_to_string(&entry).map_err(serde::de::Error::custom)?;
                    let (key, value) = entry
                        .split_once('=')
                        .map(|(key, value)| (key.to_owned(), Some(value.to_owned())))
                        .unwrap_or((entry, None));
                    output.insert(key, value);
                }
            }
            _ => return Err(serde::de::Error::custom("expected a mapping or list")),
        }
        Ok(Self(output))
    }
}

impl Serialize for StringMap {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[derive(Clone, Debug, Default)]
pub struct StringList(pub Vec<String>);

impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        let entries = match value {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::String(value)) => vec![value],
            Some(Value::Sequence(values)) => values
                .iter()
                .map(scalar_to_string)
                .collect::<Result<Vec<_>>>()
                .map_err(serde::de::Error::custom)?,
            Some(_) => return Err(serde::de::Error::custom("expected a string or list")),
        };
        Ok(Self(entries))
    }
}

impl Serialize for StringList {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[derive(Clone, Debug, Default)]
pub struct NameSet(pub Vec<String>);

impl<'de> Deserialize<'de> for NameSet {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        let names = match value {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Sequence(values)) => values
                .iter()
                .map(scalar_to_string)
                .collect::<Result<Vec<_>>>()
                .map_err(serde::de::Error::custom)?,
            Some(Value::Mapping(values)) => values
                .keys()
                .map(scalar_to_string)
                .collect::<Result<Vec<_>>>()
                .map_err(serde::de::Error::custom)?,
            Some(_) => return Err(serde::de::Error::custom("expected a list or mapping")),
        };
        Ok(Self(names))
    }
}

impl Serialize for NameSet {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DependsOn(pub Vec<String>);

impl<'de> Deserialize<'de> for DependsOn {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        let names = match value {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Sequence(values)) => values
                .iter()
                .map(scalar_to_string)
                .collect::<Result<Vec<_>>>()
                .map_err(serde::de::Error::custom)?,
            Some(Value::Mapping(values)) => values
                .keys()
                .map(scalar_to_string)
                .collect::<Result<Vec<_>>>()
                .map_err(serde::de::Error::custom)?,
            Some(_) => return Err(serde::de::Error::custom("expected a list or mapping")),
        };
        Ok(Self(names))
    }
}

impl Serialize for DependsOn {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

pub fn scalar_to_string(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        _ => bail!("expected a scalar value"),
    }
}

pub fn dependency_order(file: &ComposeFile, selected: &[String]) -> Result<Vec<String>> {
    let roots: Vec<String> = if selected.is_empty() {
        file.services.keys().cloned().collect()
    } else {
        selected.to_vec()
    };
    let mut output = Vec::new();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for service in roots {
        visit(file, &service, &mut visiting, &mut visited, &mut output)?;
    }
    Ok(output)
}

fn visit(
    file: &ComposeFile,
    name: &str,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    output: &mut Vec<String>,
) -> Result<()> {
    if visited.contains(name) {
        return Ok(());
    }
    let service = file
        .services
        .get(name)
        .with_context(|| format!("unknown service or dependency: {name}"))?;
    if !visiting.insert(name.to_owned()) {
        bail!("dependency cycle detected at service {name}");
    }
    for dependency in &service.depends_on.0 {
        visit(file, dependency, visiting, visited, output)?;
    }
    visiting.remove(name);
    visited.insert(name.to_owned());
    output.push(name.to_owned());
    Ok(())
}

pub fn load_environment(
    compose_dir: &Path,
    env_files: &[PathBuf],
) -> Result<HashMap<String, String>> {
    let mut environment = HashMap::new();
    let default = compose_dir.join(".env");
    if default.exists() {
        read_env_file(&default, &mut environment)?;
    }
    for path in env_files {
        read_env_file(path, &mut environment)?;
    }
    environment.extend(std::env::vars());
    Ok(environment)
}

pub fn read_env_file(path: &Path, output: &mut HashMap<String, String>) -> Result<()> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read environment file {}", path.display()))?;
    for (line_number, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, value) = line.split_once('=').with_context(|| {
            format!(
                "invalid environment entry at {}:{}",
                path.display(),
                line_number + 1
            )
        })?;
        let value = value.trim();
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        output.insert(key.trim().to_owned(), value.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_dependencies_before_dependents() {
        let file: ComposeFile = serde_yaml::from_str(
            "services:\n  web:\n    image: web\n    depends_on: [db]\n  db:\n    image: db\n",
        )
        .unwrap();
        assert_eq!(
            dependency_order(&file, &["web".to_owned()]).unwrap(),
            ["db", "web"]
        );
    }

    #[test]
    fn detects_dependency_cycles() {
        let file: ComposeFile = serde_yaml::from_str(
            "services:\n  one:\n    depends_on: [two]\n  two:\n    depends_on: [one]\n",
        )
        .unwrap();
        assert!(dependency_order(&file, &[])
            .unwrap_err()
            .to_string()
            .contains("cycle"));
    }
}
