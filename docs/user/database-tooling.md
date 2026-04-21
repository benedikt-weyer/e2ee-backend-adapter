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
