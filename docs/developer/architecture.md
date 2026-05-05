# Architecture

The repository is split into two crates:

- `adapter-core`: the main `e2ee-backend-adapter` package containing the library runtime and the server binary for REST and GraphQL surfaces
- `adapter-cli`: explicit tooling commands for manifest and database workflows

PostgreSQL is the first supported database backend, but database-facing logic is isolated in traits so later engines can be added behind the same runtime surface.
