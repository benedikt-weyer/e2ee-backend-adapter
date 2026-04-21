# Database Tooling

The adapter does not create tables automatically.

Instead it provides tooling to:

- export the schema it expects
- compare a live PostgreSQL database against those expectations
- write the result to files for review

The current CLI scaffold already supports:

- `validate-manifest`
- `export-expected-schema`
- `diff`

`diff` now connects to PostgreSQL and compares the manifest against the live `public` schema.

Example:

```bash
e2ee-backend-adapter-cli diff \
	--manifest ./generated/e2ee-backend.manifest.json \
	--database-url postgres://postgres:postgres@localhost:5432/app \
	--out ./generated/schema-diff.json
```
