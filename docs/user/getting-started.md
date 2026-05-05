# Getting Started

The primary integration path is:

1. generate a backend adapter manifest from `e2ee-client-backend`
2. point `e2ee-backend-adapter` at that manifest
3. configure `DATABASE_URL`
4. run the Rust REST server

Example:

```bash
e2ee-backend-adapter-server \
  --manifest ./generated/e2ee-backend.manifest.json \
  --database-url postgres://postgres:postgres@localhost:5432/app
```

The current scaffold validates the manifest, connects to PostgreSQL, creates the auth tables it needs if they are missing, and exposes generated route placeholders plus adapter metadata endpoints.

Before wiring this into a larger project, read [File Lifecycle](file-lifecycle.md).
That page explains which files you author, which files are generated, who
consumes each one, and which ones should not be edited by hand.

For generated client schema files, use the CLI export command:

```bash
e2ee-backend-adapter-cli export-expected-schema \
  --schema-config ./e2ee-backend.schema-config.json \
  --api graphql \
  --out ./generated/expected-schema.json \
  --typescript-out ./generated/e2ee-client-bindings.ts
```

The schema config is now the backend-owned source of truth for generated client
bindings. It includes the full logical entity structure, both encrypted and
non-encrypted fields. Use `--api graphql` or `--api rest` to choose the client
binding surface to export.

If you want a starter schema config from a live Postgres database, scaffold one
first and then refine it manually:

```bash
e2ee-backend-adapter-cli generate-schema-config \
  --database-url postgres://postgres:postgres@localhost:5432/app \
  --name my-backend \
  --out ./e2ee-backend.schema-config.json
```

The scaffold command is intentionally a starting point. It can infer tables,
columns, primary keys, and encrypted ciphertext/nonce pairs, but it cannot know
the final logical client shape you want for decrypted objects.

For database reconciliation workflows, use `diff`, which now writes SQL by
default:

```bash
e2ee-backend-adapter-cli diff \
  --manifest ./generated/e2ee-backend.manifest.json \
  --database-url postgres://postgres:postgres@localhost:5432/app \
  --out ./generated/schema-diff.sql
```

If you prefer a SeaORM migration scaffold instead of plain SQL:

```bash
e2ee-backend-adapter-cli diff \
  --format seaorm \
  --manifest ./generated/e2ee-backend.manifest.json \
  --database-url postgres://postgres:postgres@localhost:5432/app \
  --out ./migration/src/m20260503_000001_sync_manifest.rs
```
