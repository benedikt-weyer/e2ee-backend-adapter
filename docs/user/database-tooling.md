# Database Tooling

The adapter does not generate full application-specific table layouts yet, but
it can compare the manifest against a live PostgreSQL schema and emit
reconciliation output.

The `diff` command now writes SQL by default.

Example:

```bash
e2ee-backend-adapter-cli diff \
	--manifest ./generated/e2ee-backend.manifest.json \
	--database-url postgres://postgres:postgres@localhost:5432/app \
	--out ./generated/schema-diff.sql
```

The generated SQL reconciles the schema guarantees the adapter currently
validates:

- creation of missing auth tables and auth indexes using the adapter's own SQL
- creation of missing entity tables with the manifest primary key column
- primary-key fixes for existing entity tables
- removal of unexpected tables with `DROP TABLE IF EXISTS ... CASCADE`

Because the adapter does not yet own full entity persistence, the generated SQL
does not attempt to infer every application-specific entity column. For missing
entity tables it creates the manifest primary-key column, which is enough to
bring the live schema back into alignment with the checks the adapter currently
performs.

If you want a SeaORM migration scaffold instead of a plain SQL file, pass
`--format seaorm`:

```bash
e2ee-backend-adapter-cli diff \
	--format seaorm \
	--manifest ./generated/e2ee-backend.manifest.json \
	--database-url postgres://postgres:postgres@localhost:5432/app \
	--out ./migration/src/m20260503_000001_sync_manifest.rs
```

That output wraps the generated SQL inside a SeaORM migration file and leaves
`down(...)` as a no-op.

If you still want the old machine-readable report, `--format json` remains
available.
