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

For generated client schema files, use the CLI export command:

```bash
e2ee-backend-adapter-cli export-expected-schema \
  --manifest ./generated/e2ee-backend.manifest.json \
  --out ./generated/expected-schema.json \
  --typescript-out ./generated/e2ee-client-bindings.ts
```

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
