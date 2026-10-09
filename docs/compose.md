# Compose plugin

The experimental Compose plugin lets you define and run multi-container applications with the Compose file format while using `container` as the runtime. The plugin is implemented in Rust and is installed as the `container compose` subcommand.

## Build

Rust 1.81 or newer is required in addition to the normal `container` build requirements.

Build only the plugin:

```bash
make rust-compose
```

Run its unit tests:

```bash
make rust-compose-tests
```

The normal `make`, packaging, and installation flows also build and install the plugin.

## Quick start

Create `compose.yaml`:

```yaml
name: example

services:
  web:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgres://app:secret@db/app
    depends_on:
      - db

  db:
    image: docker.io/library/postgres:17
    environment:
      POSTGRES_USER: app
      POSTGRES_PASSWORD: secret
      POSTGRES_DB: app
    volumes:
      - database:/var/lib/postgresql/data

volumes:
  database: {}
```

Start the container system, validate the file, and launch the project:

```bash
container system start
container compose config
container compose up --build --detach
container compose ps
container compose logs --follow
```

Stop the application while keeping its named volumes:

```bash
container compose down
```

Pass `--volumes` to remove project-owned named volumes as well:

```bash
container compose down --volumes
```

## Commands

The initial implementation provides:

- `config` to validate, interpolate, and render the Compose model.
- `build` to build service images.
- `pull` to fetch service images.
- `up` to create project resources and start services in dependency order.
- `down` to stop services in reverse dependency order and remove project resources.
- `ps` to list project service containers.
- `logs` to read or follow service logs.
- `exec` to run a command in a service container.

Global options include `--file`, `--project-name`, `--env-file`, `--verbose`, and `--dry-run`. Set `CONTAINER_COMPOSE_BACKEND` to override the `container` executable, which is useful for testing wrappers and development builds.

## Supported Compose fields

The first version supports these service fields:

- `image`, `build`, `command`, and scalar `entrypoint`
- `environment` and `env_file`
- `depends_on`
- `ports`, `volumes`, and `networks`
- `working_dir`, `user`, and `platform`
- `cpus`, `mem_limit`, and `shm_size`
- `labels`, `cap_add`, and `cap_drop`
- `read_only`, `tmpfs`, `init`, `stdin_open`, and `tty`

It also supports top-level named volumes and networks, including `name` and `external`. Relative bind mounts, build contexts, Dockerfiles, and environment files are resolved relative to the Compose file.

Variable interpolation supports `${VAR}`, `${VAR:-default}`, `${VAR-default}`, `${VAR:?error}`, `${VAR?error}`, and `$$`. Values from the host environment override values loaded from `.env` and explicitly supplied environment files.

## Current limitations

This is an experimental compatibility layer, not yet a complete implementation of the Compose specification.

- `up` starts services in the background. Without `--detach`, it prints guidance for attaching with `logs --follow`.
- Service scaling and replica indexes other than `1` are not implemented.
- Compose file merging through multiple `--file` arguments is not implemented.
- Long-form port definitions, configs, secrets, profiles, health-check conditions, and deploy settings are not implemented.
- List-valued entrypoints are rejected because the current `container run --entrypoint` interface accepts one executable value.
- Changes are applied by recreating selected project containers.

Unsupported service fields produce warnings. Invalid references, dependency cycles, missing external resources, and unsupported mount forms fail before a service is started.

