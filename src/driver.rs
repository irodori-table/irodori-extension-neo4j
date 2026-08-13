use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use neo4rs::{query as cypher, ConfigBuilder, Graph};
use serde_json::{json, Map, Value};
use tokio::runtime::Runtime;

use crate::abi::{self, IrodoriConnectorBuffer};
use crate::{ABI_VERSION, CONFIG_JSON, DRIVER_LINKED, ENGINE, MANIFEST_JSON};

static CONNECTIONS: OnceLock<Mutex<HashMap<String, GraphConnection>>> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

#[derive(Clone)]
struct GraphConnection {
    graph: Graph,
    config: GraphConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GraphConfig {
    uri: String,
    username: String,
    password: String,
    database: String,
    redaction_values: Vec<String>,
    /// Path to the client certificate, when the profile supplies one.
    client_certificate: Option<String>,
}

type QueryRows = Vec<Vec<Value>>;
type QueryOutput = (Vec<String>, QueryRows, bool);

fn connections() -> &'static Mutex<HashMap<String, GraphConnection>> {
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime() -> Result<&'static Runtime, String> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = Runtime::new().map_err(|err| format!("create tokio runtime failed: {err}"))?;
    let _ = RUNTIME.set(runtime);
    RUNTIME
        .get()
        .ok_or_else(|| "create tokio runtime failed.".to_string())
}

pub fn call_json(request: IrodoriConnectorBuffer) -> IrodoriConnectorBuffer {
    let request = match abi::parse_request(request) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let method = match abi::request_method(request.as_ref()) {
        Ok(method) => method,
        Err(response) => return response,
    };

    match method {
        "health" | "ping" => abi::ok(Map::from_iter([
            ("engine".to_string(), Value::String(ENGINE.to_string())),
            ("abiVersion".to_string(), json!(ABI_VERSION)),
            ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
        ])),
        "describe" | "capabilities" => abi::ok(Map::from_iter([
            ("engine".to_string(), Value::String(ENGINE.to_string())),
            ("abiVersion".to_string(), json!(ABI_VERSION)),
            ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
            (
                "manifest".to_string(),
                serde_json::from_str(MANIFEST_JSON).unwrap_or(Value::Null),
            ),
            (
                "config".to_string(),
                serde_json::from_str(CONFIG_JSON).unwrap_or(Value::Null),
            ),
        ])),
        "manifest" => abi::owned_buffer(MANIFEST_JSON.to_string()),
        "config" => abi::owned_buffer(CONFIG_JSON.to_string()),
        "connect" => connect(request.as_ref().expect("connect has request")),
        "query" => query(request.as_ref().expect("query has request")),
        "metadata" => metadata(request.as_ref().expect("metadata has request")),
        "close" => close(request.as_ref().expect("close has request")),
        other => abi::error(
            "connector.unknownMethod",
            format!("unknown connector method: {other}"),
        ),
    }
}

fn connect(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let config = match GraphConfig::from_request(request) {
        Ok(config) => config,
        Err(err) => return abi::error("connector.invalidRequest", err),
    };
    let connection =
        match runtime().and_then(|runtime| runtime.block_on(GraphConnection::new(config))) {
            Ok(connection) => connection,
            Err(err) => return abi::error("connector.connectFailed", err),
        };
    let version = runtime()
        .ok()
        .and_then(|runtime| runtime.block_on(load_version(&connection)).ok())
        .unwrap_or_else(|| ENGINE.to_string());
    let mut guard = match connections().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abi::error(
                "connector.statePoisoned",
                "Connector connection state is poisoned.",
            )
        }
    };
    let response = Map::from_iter([
        ("engine".to_string(), Value::String(ENGINE.to_string())),
        (
            "connectionId".to_string(),
            Value::String(connection_id.clone()),
        ),
        ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
        (
            "database".to_string(),
            Value::String(connection.config.database.clone()),
        ),
        ("serverVersion".to_string(), Value::String(version)),
    ]);
    guard.insert(connection_id, connection);
    abi::ok(response)
}

fn query(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let Some(statement) = abi::string_field(request, "cypher")
        .or_else(|| abi::string_field(request, "sql"))
        .or_else(|| abi::string_field(request, "query"))
        .or_else(|| abi::string_field(request, "statement"))
    else {
        return abi::error(
            "connector.invalidRequest",
            "query requires a string cypher, sql, query, or statement field.",
        );
    };
    let connection = match connection(&connection_id) {
        Ok(connection) => connection,
        Err(response) => return response,
    };
    match runtime().and_then(|runtime| {
        runtime.block_on(run_cypher(&connection, statement, abi::max_rows(request)))
    }) {
        Ok((columns, rows, truncated)) => abi::ok(Map::from_iter([
            ("connectionId".to_string(), Value::String(connection_id)),
            (
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::String).collect()),
            ),
            (
                "rows".to_string(),
                Value::Array(rows.into_iter().map(Value::Array).collect()),
            ),
            ("truncated".to_string(), Value::Bool(truncated)),
        ])),
        Err(err) => abi::error("connector.queryFailed", connection.config.redact(&err)),
    }
}

fn metadata(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let connection = match connection(&connection_id) {
        Ok(connection) => connection,
        Err(response) => return response,
    };
    match runtime().and_then(|runtime| runtime.block_on(load_metadata(&connection))) {
        Ok(metadata) => abi::ok(Map::from_iter([
            ("connectionId".to_string(), Value::String(connection_id)),
            ("metadata".to_string(), metadata),
        ])),
        Err(err) => abi::error("connector.metadataFailed", connection.config.redact(&err)),
    }
}

fn close(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let mut guard = match connections().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abi::error(
                "connector.statePoisoned",
                "Connector connection state is poisoned.",
            )
        }
    };
    let existed = guard.remove(&connection_id).is_some();
    abi::ok(Map::from_iter([
        ("connectionId".to_string(), Value::String(connection_id)),
        ("closed".to_string(), Value::Bool(existed)),
    ]))
}

impl GraphConnection {
    async fn new(config: GraphConfig) -> Result<Self, String> {
        let mut builder = ConfigBuilder::default()
            .uri(&config.uri)
            .user(&config.username)
            .password(&config.password)
            .db(config.database.as_str());
        // The certificate authenticating this client to the server, as
        // `connector.config.json` declares under `clientCertificate`. A path,
        // never key material: connector options persist in the clear.
        if let Some(path) = config.client_certificate.as_deref() {
            // neo4rs reads the file when it builds the config, and reports a
            // missing one as a generic config failure. Check first so the error
            // names the field and the path.
            if !std::path::Path::new(path).exists() {
                return Err(format!("SSL client certificate at {path} does not exist."));
            }
            builder = builder.with_client_certificate(path);
        }
        let neo_config = builder
            .build()
            .map_err(|err| format!("failed to build {ENGINE} config: {err}"))?;
        let graph = Graph::connect(neo_config)
            .await
            .map_err(|err| format!("failed to connect to {ENGINE}: {err}"))?;
        Ok(Self { graph, config })
    }
}

impl GraphConfig {
    fn from_request(request: &Value) -> Result<Self, String> {
        let uri =
            option_string(request, &["connectionString", "url", "dsn"]).unwrap_or_else(|| {
                let host = option_string(request, &["host", "endpoint"])
                    .unwrap_or_else(|| "127.0.0.1".into());
                let port = option_string(request, &["port"]).unwrap_or_else(|| "7687".into());
                format!("bolt://{host}:{port}")
            });
        let username = option_string(request, &["user", "username"]).unwrap_or_else(|| {
            if ENGINE == "memgraph" {
                String::new()
            } else {
                "neo4j".into()
            }
        });
        let password = option_string(request, &["password", "token"]).unwrap_or_default();
        let database = option_string(request, &["database", "db"]).unwrap_or_else(|| {
            if ENGINE == "memgraph" {
                "memgraph".into()
            } else {
                "neo4j".into()
            }
        });
        let client_certificate = option_string(
            request,
            &[
                "sslCert",
                "sslcert",
                "ssl-cert",
                "clientCert",
                "clientCertificate",
            ],
        );
        let mut redaction_values = Vec::new();
        push_sensitive(&mut redaction_values, Some(&password));
        collect_url_auth(&uri, &mut redaction_values);
        Ok(Self {
            uri,
            username,
            password,
            database,
            client_certificate,
            redaction_values,
        })
    }

    fn redact(&self, message: &str) -> String {
        self.redaction_values.iter().fold(
            message.replace(&self.uri, "<graph-uri>"),
            |message, secret| {
                if secret.is_empty() {
                    message
                } else {
                    message.replace(secret, "****")
                }
            },
        )
    }
}

async fn load_version(connection: &GraphConnection) -> Result<String, String> {
    let statement = if ENGINE == "memgraph" {
        "SHOW VERSION INFO"
    } else {
        "CALL dbms.components() YIELD name, versions, edition RETURN versions[0] AS version, edition LIMIT 1"
    };
    let (columns, rows, _) = run_cypher(connection, statement, 1).await?;
    if ENGINE == "memgraph" {
        return Ok("Memgraph".to_string());
    }
    let version_idx = columns.iter().position(|column| column == "version");
    let edition_idx = columns.iter().position(|column| column == "edition");
    let version = version_idx
        .and_then(|idx| rows.first().and_then(|row| row.get(idx)))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let edition = edition_idx
        .and_then(|idx| rows.first().and_then(|row| row.get(idx)))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Ok(format!("Neo4j {version} ({edition})"))
}

async fn run_cypher(
    connection: &GraphConnection,
    statement: &str,
    cap: usize,
) -> Result<QueryOutput, String> {
    let mut result = connection
        .graph
        .execute(cypher(statement))
        .await
        .map_err(|err| format!("{ENGINE} query execution failed: {err}"))?;
    let mut columns = Vec::new();
    let mut records = Vec::<BTreeMap<String, Value>>::new();
    let mut truncated = false;
    while let Some(row) = result
        .next()
        .await
        .map_err(|err| format!("failed to fetch {ENGINE} row: {err}"))?
    {
        if records.len() >= cap {
            truncated = true;
            break;
        }
        let record = row
            .to::<BTreeMap<String, Value>>()
            .map_err(|err| format!("failed to decode {ENGINE} row: {err}"))?;
        for key in record.keys() {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
        records.push(record);
    }
    let rows = records
        .into_iter()
        .map(|record| {
            columns
                .iter()
                .map(|column| record.get(column).cloned().unwrap_or(Value::Null))
                .collect()
        })
        .collect();
    Ok((columns, rows, truncated))
}

async fn load_metadata(connection: &GraphConnection) -> Result<Value, String> {
    let (label_columns, label_rows, _) = run_cypher(
        connection,
        "CALL db.labels() YIELD label RETURN label",
        1_000,
    )
    .await?;
    let label_idx = label_columns.iter().position(|column| column == "label");
    let mut objects = Vec::new();
    if let Some(label_idx) = label_idx {
        for row in label_rows {
            if let Some(label) = row.get(label_idx).and_then(Value::as_str) {
                objects.push(label_metadata(connection, label, "nodeLabel").await?);
            }
        }
    }

    let (rel_columns, rel_rows, _) = run_cypher(
        connection,
        "CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType",
        1_000,
    )
    .await
    .unwrap_or_default();
    if let Some(rel_idx) = rel_columns
        .iter()
        .position(|column| column == "relationshipType")
    {
        for row in rel_rows {
            if let Some(rel_type) = row.get(rel_idx).and_then(Value::as_str) {
                objects.push(relationship_metadata(connection, rel_type).await?);
            }
        }
    }

    Ok(json!({
        "schemas": [{
            "name": connection.config.database,
            "objects": objects
        }]
    }))
}

async fn label_metadata(
    connection: &GraphConnection,
    label: &str,
    kind: &str,
) -> Result<Value, String> {
    let statement = format!(
        "MATCH (n:`{}`) UNWIND keys(n) AS key RETURN DISTINCT key LIMIT 100",
        escape_name(label)
    );
    let (columns, rows, _) = run_cypher(connection, &statement, 100).await?;
    Ok(property_metadata_object(
        &connection.config.database,
        label,
        kind,
        &columns,
        rows,
    ))
}

async fn relationship_metadata(
    connection: &GraphConnection,
    rel_type: &str,
) -> Result<Value, String> {
    let statement = format!(
        "MATCH ()-[r:`{}`]->() UNWIND keys(r) AS key RETURN DISTINCT key LIMIT 100",
        escape_name(rel_type)
    );
    let (columns, rows, _) = run_cypher(connection, &statement, 100).await?;
    Ok(property_metadata_object(
        &connection.config.database,
        rel_type,
        "relationshipType",
        &columns,
        rows,
    ))
}

fn property_metadata_object(
    database: &str,
    name: &str,
    kind: &str,
    columns: &[String],
    rows: QueryRows,
) -> Value {
    let key_idx = columns.iter().position(|column| column == "key");
    let properties = key_idx
        .map(|idx| {
            rows.into_iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    row.get(idx).and_then(Value::as_str).map(|name| {
                        json!({
                            "name": name,
                            "dataType": "property",
                            "nullable": true,
                            "ordinal": index + 1
                        })
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "schema": database,
        "name": name,
        "kind": kind,
        "columns": properties,
        "indexes": [],
        "primaryKey": [],
        "foreignKeys": []
    })
}

fn escape_name(name: &str) -> String {
    name.replace('`', "``")
}

fn connection(connection_id: &str) -> Result<GraphConnection, IrodoriConnectorBuffer> {
    let guard = connections().lock().map_err(|_| {
        abi::error(
            "connector.statePoisoned",
            "Connector connection state is poisoned.",
        )
    })?;
    guard.get(connection_id).cloned().ok_or_else(|| {
        abi::error(
            "connector.connectionNotFound",
            format!("no open connection: {connection_id}"),
        )
    })
}

fn request_containers(request: &Value) -> Vec<&Value> {
    [
        Some(request),
        request.get("profile"),
        request.get("options"),
        request.get("auth"),
        request.get("secrets"),
        request
            .get("profile")
            .and_then(|profile| profile.get("options")),
        request
            .get("profile")
            .and_then(|profile| profile.get("auth")),
        request
            .get("profile")
            .and_then(|profile| profile.get("secrets")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn option_string(request: &Value, fields: &[&str]) -> Option<String> {
    request_containers(request)
        .into_iter()
        .find_map(|container| {
            fields.iter().find_map(|field| {
                container
                    .get(*field)
                    .map(|value| match value {
                        Value::String(value) => value.clone(),
                        Value::Number(value) => value.to_string(),
                        Value::Bool(value) => value.to_string(),
                        _ => String::new(),
                    })
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
        })
}

fn push_sensitive(values: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        if !values.iter().any(|existing| existing == value) {
            values.push(value.to_string());
        }
    }
}

fn collect_url_auth(url: &str, values: &mut Vec<String>) {
    let Some(after_scheme) = url.split_once("://").map(|(_, rest)| rest) else {
        return;
    };
    let Some(auth) = after_scheme
        .split('/')
        .next()
        .and_then(|host| host.split('@').next())
    else {
        return;
    };
    if auth.contains(':') {
        for part in auth.split(':') {
            push_sensitive(values, Some(part));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_config_from_profile() {
        let request = json!({
            "profile": {
                "host": "graph.local",
                "port": 7688,
                "database": "neo4j",
                "user": "neo4j",
                "password": "secret"
            }
        });
        let config = GraphConfig::from_request(&request).unwrap();
        assert_eq!(config.uri, "bolt://graph.local:7688");
        assert_eq!(config.username, "neo4j");
        assert_eq!(config.database, "neo4j");
    }

    #[test]
    fn builds_property_metadata_object() {
        let columns = vec!["key".to_string()];
        let rows = vec![vec![json!("name")], vec![json!("age")]];
        let metadata = property_metadata_object("neo4j", "Person", "nodeLabel", &columns, rows);
        assert_eq!(metadata["name"], "Person");
        assert_eq!(metadata["columns"][1]["name"], "age");
    }

    #[test]
    fn reads_the_client_certificate_path_from_the_connector_options() {
        for field in ["sslCert", "ssl-cert", "clientCert", "clientCertificate"] {
            let config = GraphConfig::from_request(&json!({
                "profile": { "host": "graph.local", "options": { field: "/etc/ssl/client.pem" } }
            }))
            .unwrap();
            assert_eq!(
                config.client_certificate.as_deref(),
                Some("/etc/ssl/client.pem"),
                "{field}"
            );
        }
    }

    #[test]
    fn a_profile_without_a_certificate_carries_none() {
        let config = GraphConfig::from_request(&json!({
            "profile": { "host": "graph.local" }
        }))
        .unwrap();
        assert_eq!(config.client_certificate, None);
    }
}
