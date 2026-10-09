# Bushel desktop app

Bushel includes a native macOS interface for managing the three core local resource types: containers, images, and volumes.

## Build and launch

```bash
make gui
open bin/Bushel.app
```

Start the runtime before opening the app if it is not already running:

```bash
bushel system start
```

The app resolves `bushel` from `PATH` by default. Set `BUSHEL_CLI_PATH` before launching, or change the executable in Settings, to use another build:

```bash
BUSHEL_CLI_PATH="$PWD/bin/bushel" open bin/Bushel.app
```

## Features

- Search across the selected resource type.
- View running state, image, allocated CPU, published ports, and creation time for containers.
- Start, stop, and remove containers.
- Inspect and remove local OCI images.
- Inspect and remove named or anonymous volumes.
- Start or stop the Bushel engine from Settings.
- Confirm destructive removal actions before executing them.

The interface reads machine-readable JSON from the same `bushel` commands available in the terminal, so the GUI and CLI share the same resource model.

For UI development without a running engine, launch the executable with `BUSHEL_GUI_DEMO=1` to populate non-persistent sample data.
