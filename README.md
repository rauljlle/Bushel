<h1>
  <img alt="Containerization logo" src="./assets/Containerization-Logo.png" width="70" valign="middle">
  &nbsp;Bushel
</h1>

Bushel is a Docker-compatible command-line experience for creating and running Linux containers as lightweight virtual machines on your Mac. The CLI is invoked as `bushel`, is written in Swift and Rust, and is optimized for Apple silicon.

The tool consumes and produces [OCI-compatible container images](https://github.com/opencontainers/image-spec), so you can pull and run images from any standard container registry. You can push images that you build to those registries as well, and run the images in any other OCI-compatible application.

Bushel uses Apple's [Containerization](https://github.com/apple/containerization) Swift package for low-level container, image, and process management.

![introductory movie showing some basic commands](./docs/assets/landing-movie.gif)

## Get started

### Requirements

You need a Mac with Apple silicon to run Bushel. To build it, see the [BUILDING](./BUILDING.md) document.

Bushel is supported on macOS 26 because its Apple container runtime takes advantage of virtualization and networking features introduced in this release.

### Initial install

Until Bushel publishes signed releases, build and install it from source by following the [building guide](./BUILDING.md).

```bash
git clone https://github.com/rauljlle/Bushel.git
cd Bushel
make install
```

Start the system service with:

```bash
bushel system start
```

### Desktop app

Build the native macOS interface and open it:

```bash
make gui
open bin/Bushel.app
```

The desktop app provides focused views for containers, images, and volumes. It uses the `bushel` executable from `PATH`; set `BUSHEL_CLI_PATH` to use a development build in another location. See the [GUI guide](./docs/gui.md) for details.

### Run your first container

```bash
bushel run --rm alpine echo hello
```

This pulls the `alpine` image, runs it in a lightweight Linux VM, prints `hello`, and
removes the container when it exits. See the [tutorial](./docs/tutorials/start-here.md)
for a fuller walkthrough that builds and publishes an image of your own.

### Upgrade or downgrade

For development builds, update the checkout and rebuild Bushel from source.

Stop Bushel before replacing an existing installation:

```bash
bushel system stop
```

To upgrade to the latest release, simply run the command below:

```bash
git pull
make install
```

To install a specific revision, check it out before rebuilding:

```bash
git checkout <revision>
make install
```

Start the system service with:

```bash
bushel system start
```

### Uninstall

The current installer retains Apple's internal runtime services and data layout. Use the bundled uninstaller to remove the installed binaries and runtime data:

```bash
/usr/local/bin/uninstall-bushel.sh -d
```

To retain your user data so that it is available should you reinstall later, run:

```bash
/usr/local/bin/uninstall-bushel.sh -k
```

## Next steps

- Take [a guided tour of Bushel](./docs/tutorials/start-here.md) by building, running, and publishing a simple web server image.
- Learn how to [use the underlying container features](./docs/how-to.md).
- Define and run multi-container applications with the experimental [Compose plugin](./docs/compose.md).
- Read a brief description and [technical overview](./docs/technical-overview.md) of the underlying runtime.
- Browse the [full command reference](./docs/command-reference.md).
- [Build and run](./BUILDING.md) Bushel on your own development system.
- View the project [API documentation](https://apple.github.io/container/documentation/).

## Contributing

Contributions to Bushel are welcome and encouraged. Please see the [contributing guide](./CONTRIBUTING.md) for more information.

## Project Status

Bushel is under active development. Its release versions are product versions, not semantic versions.

The `bushel` CLI aims to preserve backward compatibility within a major release. Features marked experimental, including Compose, may change while their compatibility surface is completed.

The `container-apiserver` XPC API compatibility preserves forward and backward compatibility within a major version. 
Other non-public XPC helpers do not guarantee CLI or API compatibility across different versions.

The `container` application data provides forward compatibility only, guaranteed within one major version. Upgrading to a newer major version may require a specific upgrade path.
