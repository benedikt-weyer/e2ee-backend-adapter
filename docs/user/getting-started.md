# Getting Started

The primary integration path is:

1. generate a backend adapter manifest from `e2ee-client-backend`
2. point `e2ee-backend-adapter` at that manifest
3. configure `DATABASE_URL`
4. run the Rust adapter server

Example:

```bash
e2ee-backend-adapter-server \
  --manifest ./generated/e2ee-backend.manifest.json \
  --database-url postgres://postgres:postgres@localhost:5432/app
```

The current runtime validates the manifest, connects to PostgreSQL, creates the auth tables it needs if they are missing, and exposes adapter metadata endpoints. When the exported expected schema uses `--api graphql`, the runtime exposes the configured GraphQL endpoint with database-backed entity CRUD plus the GraphQL auth operations. When the exported expected schema uses `--api rest`, the generated REST entity routes are also backed by the same database CRUD layer.

Before wiring this into a larger project, read [File Lifecycle](file-lifecycle.md).
That page explains which files you author, which files are generated, who
consumes each one, and which ones should not be edited by hand.

For generated client schema files, use the CLI export command:

```bash
e2ee-backend-adapter-cli export-expected-schema \
  --db-schema-config ./e2ee-backend.db-schema.json \
  --encrypted-schema-config ./e2ee-backend.encrypted-schema.json \
  --api graphql \
  --out ./generated/expected-schema.json \
  --typescript-out ./generated/e2ee-client-bindings.ts
```

The DB schema config is the structural source of truth for export generation.
It is intended to be generated from the database and contains entity names,
table mappings, inferred field shapes, and encrypted field placeholders. The
encrypted schema config is the user-authored overlay for encrypted field
structure and API naming overrides. Use `--api graphql` or `--api rest` to
choose the client binding surface to export.

If you want a DB schema config from a live Postgres database, generate it with
the CLI:

```bash
e2ee-backend-adapter-cli generate-db-schema-config \
  --database-url postgres://postgres:postgres@localhost:5432/app \
  --name my-backend \
  --out ./e2ee-backend.db-schema.json
```

The DB scaffold can infer tables, columns, primary keys, and encrypted
ciphertext/nonce pairs, but it cannot know the final logical client shape you
want for decrypted objects. That richer structure belongs in
`e2ee-backend.encrypted-schema.json`.

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
