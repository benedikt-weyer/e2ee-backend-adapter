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

The current scaffold validates the manifest, connects to PostgreSQL, and exposes generated route placeholders plus adapter metadata endpoints.

For database verification workflows, use the CLI:

```bash
e2ee-backend-adapter-cli export-expected-schema \
  --manifest ./generated/e2ee-backend.manifest.json \
  --out ./generated/expected-schema.json

e2ee-backend-adapter-cli diff \
  --manifest ./generated/e2ee-backend.manifest.json \
  --database-url postgres://postgres:postgres@localhost:5432/app \
  --out ./generated/schema-diff.json
```
