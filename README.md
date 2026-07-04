# Neo4j Connector

Native Irodori Table connector extension for Neo4j.

This crate packages the connector metadata, native ABI exports, and driver implementation used by the Irodori extension marketplace.

## Connector

- Extension ID: `irodori.neo4j`
- Engine ID: `neo4j`
- Wire protocol: `neo4j`
- Default port: `7687`
- Native ABI: `irodori.connector.native.v1`
- Driver linked: `yes`
- Marketplace visibility: `public`
- Package version: `0.1.2`

The package includes a desktop adapter source snapshot from `db/neo4j.rs`.

Connector metadata lives in `connector.config.json` and `irodori.extension.json`.
The Rust crate exports the native ABI from `src/lib.rs`, uses `irodori-connector-abi` for shared JSON/buffer helpers, and keeps connector behavior in `src/driver.rs`.

## Connection Metadata

- Endpoint modes: `hostPort`, `connectionString`
- Transport modes: `direct`, `sshTunnel`, `socks5Proxy`, `httpConnectProxy`, `proxyChain`
- TLS supported: `yes`
- TLS required by default: `no`
- Custom driver options: `yes`

### Endpoint Fields

| Field | Label | Type | Required |
| --- | --- | --- | --- |
| `host` | Host | `string` | yes |
| `port` | Port | `number` | no |
| `database` | Database | `string` | no |

## Authentication

The connector advertises these authentication modes so clients can render the right credential fields. Driver-specific or provider-specific values can still be passed through `options` when needed.

| Auth method | Label | Kind | Secret purposes |
| --- | --- | --- | --- |
| `none` | No authentication | `none` | none |
| `connectionString` | Connection string / DSN | `connectionString` | none |
| `basic` | Basic authentication | `userPassword` | `password` |
| `kerberos` | Kerberos / GSSAPI | `kerberos` | `token` |
| `bearerToken` | Bearer token | `token` | `token` |
| `clientCertificate` | Client certificate / mTLS | `certificate` | `privateKey`, `privateKeyPassphrase` |
| `customDriverOptions` | Custom driver options | `custom` | `password`, `token`, `privateKey`, `privateKeyPassphrase` |

## Experience Metadata

- Domains: `graph`
- Result views: `graph`, `path`, `table`
- Object types: `nodeLabels`, `relationshipTypes`, `properties`, `indexes`, `constraints`
- Inspired by: Neo4j Browser, Neo4j Bloom, Neo4j Graph Data Science, Cypher shortest path

| Workflow | Result view | Templates |
| --- | --- | --- |
| Schema overview | `table` | `graph-cypher-label-counts` |
| Explore neighborhood | `graph` | `graph-cypher-neighborhood` |
| Shortest path | `path` | `graph-cypher-shortest-path` |
| Algorithm starter | `table` | `graph-cypher-degree-centrality` |

| Template | Label | Language | Result view |
| --- | --- | --- | --- |
| `graph-cypher-label-counts` | Label counts | `cypher` | `table` |
| `graph-cypher-neighborhood` | Neighborhood graph | `cypher` | `graph` |
| `graph-cypher-shortest-path` | Shortest path | `cypher` | `path` |
| `graph-cypher-degree-centrality` | Degree centrality starter | `cypher` | `table` |

## Native ABI Calls

| Method | Response |
| --- | --- |
| `health` | Returns connector health, engine id, ABI version, and driver status. |
| `describe` | Returns the embedded manifest and connector config. |
| `manifest` | Returns raw `irodori.extension.json`. |
| `config` | Returns raw `connector.config.json`. |
| `connect` | Opens and validates a native connector connection. |
| `query` | Runs a connector query and returns structured rows or JSON results. |
| `metadata` | Reads schemas, tables, columns, indexes, collections, or equivalent metadata. |
| `close` | Closes and removes a cached native connection. |

## Development

All extension crates in this checkout share `../target` so dependencies compile once across sibling repositories.

```sh
make check
make build
```

Release packages place platform-specific native artifacts under `dist/native`.
