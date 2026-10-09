# Basic Compose example

Start the container system and launch the example from this directory:

```bash
container system start
container compose up --detach
curl http://localhost:8080
```

Inspect and remove the project:

```bash
container compose ps
container compose down --volumes
```

