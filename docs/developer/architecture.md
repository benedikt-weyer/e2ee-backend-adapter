# Architecture

The repository is split into three crates:

- `adapter-core`: manifest parsing, runtime state, route generation scaffolding, DB abstractions
- `adapter-server`: the primary REST server binary
- `adapter-cli`: explicit tooling commands for manifest and database workflows

PostgreSQL is the first supported database backend, but database-facing logic is isolated in traits so later engines can be added behind the same runtime surface.
