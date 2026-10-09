# Basic Compose example

Start the container system and launch the example from this directory:

```bash
bushel system start
bushel compose up --detach
curl http://localhost:8080
```

Inspect and remove the project:

```bash
bushel compose ps
bushel compose down --volumes
```
