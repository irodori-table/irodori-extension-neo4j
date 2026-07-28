<!-- i18n: language-switcher -->
[English](README.md) | [日本語](README.ja.md)

# Neo4j コネクタ

Neo4j 用のネイティブ Irodori Table コネクタ拡張です。

このクレートは、Irodori 拡張マーケットプレイスで使用されるコネクタのメタデータ、ネイティブ ABI エクスポート、およびドライバー実装をパッケージ化しています。

## コネクタ

- 拡張 ID: `irodori.neo4j`
- エンジン ID: `neo4j`
- ワイヤープロトコル: `neo4j`
- デフォルトポート: `7687`
- ネイティブ ABI: `irodori.connector.native.v1`
- ドライバー連携: `yes`
- マーケットプレイス公開範囲: `public`
- パッケージバージョン: `0.1.3`

パッケージには `db/neo4j.rs` からのデスクトップアダプターのソーススナップショットが含まれています。

コネクタのメタデータは `connector.config.json` と `irodori.extension.json` にあります。
Rust クレートは `src/lib.rs` からネイティブ ABI をエクスポートし、共有 JSON/バッファヘルパーに `irodori-connector-abi` を使用し、コネクタの動作は `src/driver.rs` に保持しています。

## 接続メタデータ

- エンドポイントモード: `hostPort`, `connectionString`
- トランスポートモード: `direct`, `sshTunnel`, `socks5Proxy`, `httpConnectProxy`, `proxyChain`
- TLS 対応: `yes`
- デフォルトで TLS 必須: `no`
- カスタムドライバーオプション: `yes`

### エンドポイントフィールド

| フィールド | ラベル | 型 | 必須 |
| --- | --- | --- | --- |
| `host` | ホスト | `string` | yes |
| `port` | ポート | `number` | no |
| `database` | データベース | `string` | no |

## 認証

コネクタはこれらの認証モードを広告しており、クライアントは適切な認証情報フィールドを表示できます。
ドライバー固有またはプロバイダー固有の値は必要に応じて `options` 経由で渡すことも可能です。

| 認証方式 | ラベル | 種類 | 秘密情報の用途 |
| --- | --- | --- | --- |
| `none` | 認証なし | `none` | なし |
| `connectionString` | 接続文字列 / DSN | `connectionString` | なし |
| `basic` | ベーシック認証 | `userPassword` | `password` |
| `kerberos` | Kerberos / GSSAPI | `kerberos` | `token` |
| `bearerToken` | ベアラートークン | `token` | `token` |
| `clientCertificate` | クライアント証明書 / mTLS | `certificate` | `privateKey`, `privateKeyPassphrase` |
| `customDriverOptions` | カスタムドライバーオプション | `custom` | `password`, `token`, `privateKey`, `privateKeyPassphrase` |

## エクスペリエンスメタデータ

- ドメイン: `graph`
- 結果ビュー: `graph`, `path`, `table`
- オブジェクトタイプ: `nodeLabels`, `relationshipTypes`, `properties`, `indexes`, `constraints`
- インスパイア元: Neo4j Browser, Neo4j Bloom, Neo4j Graph Data Science, Cypher shortest path

| ワークフロー | 結果ビュー | テンプレート |
| --- | --- | --- |
| スキーマ概要 | `table` | `graph-cypher-label-counts` |
| 近隣探索 | `graph` | `graph-cypher-neighborhood` |
| 最短経路 | `path` | `graph-cypher-shortest-path` |
| アルゴリズムスターター | `table` | `graph-cypher-degree-centrality` |

| テンプレート | ラベル | 言語 | 結果ビュー |
| --- | --- | --- | --- |
| `graph-cypher-label-counts` | ラベルカウント | `cypher` | `table` |
| `graph-cypher-neighborhood` | 近隣グラフ | `cypher` | `graph` |
| `graph-cypher-shortest-path` | 最短経路 | `cypher` | `path` |
| `graph-cypher-degree-centrality` | 次数中心性スターター | `cypher` | `table` |

## ネイティブ ABI コール

| メソッド | レスポンス |
| --- | --- |
| `health` | コネクタのヘルス、エンジン ID、ABI バージョン、ドライバー状態を返します。 |
| `describe` | 埋め込みマニフェストとコネクタ設定を返します。 |
| `manifest` | 生の `irodori.extension.json` を返します。 |
| `config` | 生の `connector.config.json` を返します。 |
| `connect` | ネイティブコネクタ接続を開き、検証します。 |
| `query` | コネクタクエリを実行し、構造化された行または JSON 結果を返します。 |
| `metadata` | スキーマ、テーブル、カラム、インデックス、コレクション、または同等のメタデータを読み取ります。 |
| `close` | キャッシュされたネイティブ接続を閉じて削除します。 |

## 開発

このチェックアウト内のすべての拡張クレートは `../target` を共有しているため、依存関係は兄弟リポジトリ間で一度だけコンパイルされます。

```sh
make check
make build
```

リリースパッケージはプラットフォーム固有のネイティブアーティファクトを `dist/native` に配置します。

## ライセンス

0BSD。ほぼあらゆる目的でこのプロジェクトを使用、コピー、修正、配布できます。