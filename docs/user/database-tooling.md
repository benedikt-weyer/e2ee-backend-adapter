# Database Tooling

The adapter does not create tables automatically.

Instead it provides tooling to:

- export the schema it expects
- compare a live PostgreSQL database against those expectations
- write the result to files for review

`export-expected-schema` exports a JSON representation of the manifest's
database expectations. It is not SQL DDL and it is not a migration file.

The exported shape is:

```json
{
	"authTables": ["users", "sessions"],
	"entityTables": [
		{
			"primaryKey": "id",
			"tableName": "notes"
		}
	]
}
```

This describes:

- auth-related tables the adapter expects to exist
- entity tables the adapter expects to exist
- the expected primary key field for each entity table

Example export workflow:

```bash
e2ee-backend-adapter-cli export-expected-schema \
	--manifest ./generated/e2ee-backend.manifest.json \
	--out ./generated/expected-schema.json
```

Example output file:

```json
{
	"authTables": ["users", "sessions"],
	"entityTables": [
		{
			"primaryKey": "id",
			"tableName": "notes"
		},
		{
			"primaryKey": "id",
			"tableName": "comments"
		}
	]
}
```

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
