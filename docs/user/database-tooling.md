# Database Tooling

The adapter does not create tables automatically.

Instead it provides tooling to:

- export the schema it expects
- compare a live PostgreSQL database against those expectations
- write the result to files for review

When you want the backend adapter to own field metadata such as which fields are
encrypted, store that metadata in a schema config file and pass it to the
adapter with `--schema-config`.

Example schema config file:

```json
{
	"expectedSchema": {
		"authTables": ["users", "sessions"],
		"entities": [
			{
				"fields": [
					{
						"encrypted": true,
						"entityPath": "content",
						"entityType": "string",
						"nullable": false,
						"optional": false,
						"remotePath": "ciphertext",
						"remoteType": "string",
						"strategyId": "aes-256-gcm"
					}
				],
				"idPath": "id",
				"name": "note",
				"primaryKey": "id",
				"tableName": "notes"
			}
		],
		"entityTables": [
			{
				"primaryKey": "id",
				"tableName": "notes"
			}
		]
	}
}
```

`export-expected-schema` exports a JSON representation of the manifest's
database expectations. It is not SQL DDL and it is not a migration file.

The export is now rich enough to drive frontend or client-side entity-schema
construction for declarative fields.

The exported shape is:

```json
{
	"authTables": ["users", "sessions"],
	"entities": [
		{
			"fields": [
				{
					"encrypted": true,
					"entityPath": "content",
					"entityType": "string",
					"nullable": false,
					"optional": false,
					"remotePath": "ciphertext",
					"remoteType": "string",
					"strategyId": "aes-256-gcm"
				},
				{
					"encrypted": false,
					"entityPath": "id",
					"entityType": "string",
					"nullable": false,
					"optional": false,
					"remotePath": "id",
					"remoteType": "string"
				}
			],
			"idPath": "id",
			"name": "note",
			"primaryKey": "id",
			"tableName": "notes"
		}
	],
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
- per-entity field metadata including logical field names, remote field names,
  data types, nullability, optionality, and whether a field is e2ee-encrypted

Example export workflow:

```bash
e2ee-backend-adapter-cli export-expected-schema \
	--manifest ./generated/e2ee-backend.manifest.json \
	--schema-config ./config/e2ee-backend.schema.json \
	--out ./generated/expected-schema.json
```

Example output file:

```json
{
	"authTables": ["users", "sessions"],
	"entities": [
		{
			"fields": [
				{
					"encrypted": true,
					"entityPath": "config",
					"entityType": "object",
					"nullable": true,
					"optional": false,
					"remotePath": "configEnvelope",
					"remoteType": "object",
					"strategyId": "aes-256-gcm"
				},
				{
					"encrypted": false,
					"entityPath": "id",
					"entityType": "string",
					"nullable": false,
					"optional": false,
					"remotePath": "id",
					"remoteType": "string"
				}
			],
			"idPath": "id",
			"name": "dashboard",
			"primaryKey": "id",
			"tableName": "dashboards"
		},
		{
			"fields": [
				{
					"encrypted": false,
					"entityPath": "id",
					"entityType": "string",
					"nullable": false,
					"optional": false,
					"remotePath": "id",
					"remoteType": "string"
				}
			],
			"idPath": "id",
			"name": "comment",
			"primaryKey": "id",
			"tableName": "comments"
		}
	],
	"entityTables": [
		{
			"primaryKey": "id",
			"tableName": "dashboards"
		},
		{
			"primaryKey": "id",
			"tableName": "comments"
		}
	]
}
```

On the client side, `e2ee-client-backend` can now reconstruct declarative
`EntitySchema` definitions from `expectedSchema.entities` without redefining
field paths, types, or encryption flags by hand.

The current CLI scaffold already supports:

- `validate-manifest`
- `export-expected-schema`
- `diff`

`diff` now connects to PostgreSQL and compares the manifest against the live `public` schema.

Example:

```bash
e2ee-backend-adapter-cli diff \
	--manifest ./generated/e2ee-backend.manifest.json \
	--schema-config ./config/e2ee-backend.schema.json \
	--database-url postgres://postgres:postgres@localhost:5432/app \
	--out ./generated/schema-diff.json
```
