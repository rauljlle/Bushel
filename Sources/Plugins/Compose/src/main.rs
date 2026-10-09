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

mod backend;
mod interpolate;
mod model;
mod project;

use anyhow::{bail, Result};
use backend::Backend;
use clap::{Args, Parser, Subcommand};
use project::Project;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "compose",
    version,
    about = "Define and run multi-container applications"
)]
struct Cli {
    /// Path to the Compose file.
    #[arg(short = 'f', long = "file", global = true)]
    file: Option<PathBuf>,

    /// Override the Compose project name.
    #[arg(short = 'p', long = "project-name", global = true)]
    project_name: Option<String>,

    /// Environment file used for Compose interpolation.
    #[arg(long = "env-file", global = true)]
    env_file: Vec<PathBuf>,

    /// Print backend commands before executing them.
    #[arg(long, global = true)]
    verbose: bool,

    /// Print the commands without changing container state.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate and render the resolved Compose model.
    Config(ConfigArgs),
    /// Build service images.
    Build(BuildArgs),
    /// Pull service images.
    Pull(ServiceSelection),
    /// Create and start services.
    Up(UpArgs),
    /// Stop and remove services and project networks.
    Down(DownArgs),
    /// List project containers.
    Ps(ServiceSelection),
    /// Display service logs.
    Logs(LogsArgs),
    /// Execute a command in a running service container.
    Exec(ExecArgs),
}

#[derive(Clone, Debug, Default, Args)]
struct ServiceSelection {
    /// Limit the operation to these services (dependencies are included where applicable).
    services: Vec<String>,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    /// Print only service names.
    #[arg(long)]
    services: bool,
}

#[derive(Debug, Args)]
struct BuildArgs {
    #[command(flatten)]
    selection: ServiceSelection,

    /// Do not use cached layers.
    #[arg(long)]
    no_cache: bool,
}

#[derive(Debug, Args)]
struct UpArgs {
    #[command(flatten)]
    selection: ServiceSelection,

    /// Accepted for Docker Compose compatibility; services always start detached.
    #[arg(short, long)]
    detach: bool,

    /// Build images before starting containers.
    #[arg(long)]
    build: bool,

    /// Pull service images before starting containers.
    #[arg(long)]
    pull: bool,
}

#[derive(Debug, Args)]
struct DownArgs {
    /// Remove project named volumes in addition to containers and networks.
    #[arg(short, long)]
    volumes: bool,
}

#[derive(Debug, Args)]
struct LogsArgs {
    #[command(flatten)]
    selection: ServiceSelection,

    /// Follow log output.
    #[arg(long)]
    follow: bool,

    /// Number of lines to show from the end of each log.
    #[arg(short = 'n', long)]
    tail: Option<usize>,
}

#[derive(Debug, Args)]
struct ExecArgs {
    /// Keep standard input open.
    #[arg(short, long)]
    interactive: bool,

    /// Allocate a TTY.
    #[arg(short, long)]
    tty: bool,

    /// Service name.
    service: String,

    /// Command and arguments to execute.
    #[arg(required = true, trailing_var_arg = true)]
    command: Vec<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("compose: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let backend = Backend::from_environment(cli.dry_run, cli.verbose);
    let project = Project::load(cli.file, cli.project_name, &cli.env_file, backend)?;
    match cli.command {
        Commands::Config(args) => config(&project, args),
        Commands::Build(args) => build(&project, args),
        Commands::Pull(selection) => pull(&project, &selection.services),
        Commands::Up(args) => up(&project, args),
        Commands::Down(args) => down(&project, args),
        Commands::Ps(selection) => ps(&project, &selection.services),
        Commands::Logs(args) => logs(&project, args),
        Commands::Exec(args) => exec(&project, args),
    }
}

fn config(project: &Project, args: ConfigArgs) -> Result<()> {
    if args.services {
        for service in project.file.services.keys() {
            println!("{service}");
        }
        return Ok(());
    }
    print!("{}", serde_yaml::to_string(&project.file)?);
    Ok(())
}

fn build(project: &Project, args: BuildArgs) -> Result<()> {
    for service in project.selected_services(&args.selection.services)? {
        project.build_service(&service, args.no_cache)?;
    }
    Ok(())
}

fn pull(project: &Project, selected: &[String]) -> Result<()> {
    for service_name in project.selected_services(selected)? {
        let service = &project.file.services[&service_name];
        match &service.image {
            Some(image) => project.backend.run(["image", "pull", image])?,
            None if service.build.is_some() => {
                eprintln!("{service_name}: skipped (image is built locally)")
            }
            None => bail!("service {service_name} has no image"),
        }
    }
    Ok(())
}

fn up(project: &Project, args: UpArgs) -> Result<()> {
    let services = project.services(&args.selection.services)?;
    if args.pull {
        pull(project, &services)?;
    }
    if args.build {
        for service in &services {
            project.build_service(service, false)?;
        }
    } else {
        for service in &services {
            let definition = &project.file.services[service];
            if definition.image.is_none() && definition.build.is_some() {
                project.build_service(service, false)?;
            }
        }
    }
    project.ensure_resources(&services)?;
    for service in services {
        project.run_service(&service)?;
        println!("Started {service}");
    }
    if !args.detach {
        eprintln!(
            "Services started in the background; use `container compose logs --follow` to attach."
        );
    }
    Ok(())
}

fn down(project: &Project, args: DownArgs) -> Result<()> {
    let mut services = project.services(&[])?;
    services.reverse();
    for service in services {
        project.stop_and_delete(&service)?;
    }
    project.remove_resources(args.volumes)
}

fn ps(project: &Project, selected: &[String]) -> Result<()> {
    let services = project.selected_services(selected)?;
    println!("SERVICE\tCONTAINER");
    for service in services {
        let container = project.container_name(&service);
        if project.backend.run_quiet(["inspect", &container])? {
            println!("{service}\t{container}");
        }
    }
    Ok(())
}

fn logs(project: &Project, args: LogsArgs) -> Result<()> {
    let services = project.selected_services(&args.selection.services)?;
    let mut children = Vec::new();
    for service in services {
        let mut command = vec![OsString::from("logs")];
        if args.follow {
            command.push(OsString::from("--follow"));
        }
        if let Some(tail) = args.tail {
            command.extend([OsString::from("-n"), OsString::from(tail.to_string())]);
        }
        command.push(OsString::from(project.container_name(&service)));
        if args.follow {
            children.push(project.backend.spawn(command)?);
        } else {
            println!("==> {service} <==");
            project.backend.run(command)?;
        }
    }
    for mut child in children {
        let status = child.wait()?;
        if !status.success() {
            bail!("one or more log followers failed");
        }
    }
    Ok(())
}

fn exec(project: &Project, args: ExecArgs) -> Result<()> {
    if !project.file.services.contains_key(&args.service) {
        bail!("unknown service: {}", args.service);
    }
    let mut command = vec![OsString::from("exec")];
    if args.interactive {
        command.push(OsString::from("--interactive"));
    }
    if args.tty {
        command.push(OsString::from("--tty"));
    }
    command.push(OsString::from(project.container_name(&args.service)));
    command.extend(args.command.into_iter().map(OsString::from));
    project.backend.run(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_line_definition_is_consistent() {
        Cli::command().debug_assert();
    }
}
