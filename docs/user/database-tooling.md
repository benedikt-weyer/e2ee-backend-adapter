# Database Tooling

The adapter does not create tables automatically.

Instead it provides tooling to:

- export the schema it expects
- compare a live PostgreSQL database against those expectations
- write the result to files for review

`export-expected-schema` exports a JSON representation of the manifest's
database expectations. It is not SQL DDL and it is not a migration file.

The generated file is for client-side consumption. The backend adapter does not
read it back at runtime.

If you also pass `--typescript-out`, the adapter writes a generated TypeScript
companion module alongside the JSON export. For now, TypeScript is the only
supported language target.

The export is now rich enough to drive frontend or client-side entity-schema
construction for declarative fields.

The exported shape is:

```json
{
	"expectedSchema": {
		"api": {
			"rest": {
				"baseUrl": "/api",
				"defaultHeaders": {
					"accept": "application/json"
				}
			},
			"type": "rest"
		},
		"authTables": ["users", "sessions"],
		"entities": [
			{
				"api": {
					"rest": {
						"allowCreate": true,
						"allowDelete": true,
						"allowGetById": true,
						"allowList": true,
						"allowUpdate": true,
						"basePath": "/entities/note"
					},
					"type": "rest"
				},
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
}
```

This describes:

- the API family this generated schema targets
- the default REST transport base URL and headers exported for generated clients
- auth-related tables the adapter expects to exist
- entity tables the adapter expects to exist
- the expected primary key field for each entity table
- per-entity default REST route metadata derived from the adapter config
- per-entity field metadata including logical field names, remote field names,
  data types, nullability, optionality, and whether a field is e2ee-encrypted

Example export workflow:

```bash
e2ee-backend-adapter-cli export-expected-schema \
	--manifest ./generated/e2ee-backend.manifest.json \
	--out ./generated/expected-schema.json \
	--typescript-out ./generated/e2ee-client-bindings.ts
```

The generated TypeScript module exports:

- `SessionUser`
- `<EntityName>Entity`, `<EntityName>RemoteRecord`, and `<EntityName>Id` type aliases
- `createRestTransport(...)`
- `createRestAuthConfig(...)`
- `createEntitySchemas(...)`
- `createRestModels(...)`
- `createRestCrudAdapters(...)`

That lets a client app import typed auth and model helpers directly instead of
rewriting `SessionUser`, entity types, REST route wiring, or default transport
configuration by hand. `createRestModels(...)` also wires the generated model
map automatically so apps can keep using `createE2eeBackend(...)` with
`models: createRestModels()`.

Example output file:

```json
{
	"expectedSchema": {
		"api": {
			"rest": {
				"baseUrl": "/api",
				"defaultHeaders": {
					"accept": "application/json"
				}
			},
			"type": "rest"
		},
		"authTables": ["users", "sessions"],
		"entities": [
			{
				"api": {
					"rest": {
						"allowCreate": true,
						"allowDelete": true,
						"allowGetById": true,
						"allowList": true,
						"allowUpdate": true,
						"basePath": "/entities/dashboard"
					},
					"type": "rest"
				},
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
				"api": {
					"rest": {
						"allowCreate": true,
						"allowDelete": true,
						"allowGetById": true,
						"allowList": true,
						"allowUpdate": true,
						"basePath": "/entities/comment"
					},
					"type": "rest"
				},
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
}
```

On the client side, `e2ee-client-backend` can now reconstruct declarative
`EntitySchema` definitions from `expectedSchema.entities` and derive default
`RestCrudAdapter` routes from the exported API metadata, without redefining
field paths, types, encryption flags, REST CRUD paths, or TypeScript entity
aliases by hand.

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
