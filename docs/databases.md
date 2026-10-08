# Supported databases

## Native drivers

| Database                   | Status | Type           | Query language                      |
| -------------------------- | ------ | -------------- | ----------------------------------- |
| PostgreSQL                 | Stable | SQL            | SQL                                 |
| MySQL / MariaDB            | Stable | SQL            | SQL                                 |
| SQLite                     | Stable | SQL (embedded) | SQL                                 |
| CockroachDB                | Stable | SQL            | SQL (Postgres wire protocol)        |
| ClickHouse                 | Stable | OLAP           | SQL (HTTP API)                      |
| MongoDB                    | Stable | Document       | JSON commands (`find`, `aggregate`) |
| Redis                      | Stable | Key-Value      | Redis commands                      |
| Cassandra / ScyllaDB       | Stable | Wide-Column    | CQL                                 |
| SQL Server / Azure SQL     | Beta   | SQL            | T-SQL                               |
| Oracle                     | Beta   | SQL            | SQL / PL/SQL                        |
| Amazon DynamoDB            | Beta   | Key-Value      | PartiQL                             |
| Elasticsearch / OpenSearch | Beta   | Search         | SQL, or Dev Tools console syntax    |
| SurrealDB                  | Beta   | Multi-model    | SurrealQL                           |

## Cloud & compatible services

These speak the wire protocol of a native driver. Choose one from the **preset** dropdown in the New Connection dialog and it fills in the right driver, port and TLS setting.

| Service                                                                                 | Driver                     |
| --------------------------------------------------------------------------------------- | -------------------------- |
| **Amazon Aurora PostgreSQL**, Aurora DSQL, RDS PostgreSQL, Redshift                     | PostgreSQL                 |
| **Amazon Aurora MySQL**, RDS MySQL / MariaDB                                            | MySQL                      |
| Amazon RDS SQL Server, **Azure SQL Database**                                           | SQL Server                 |
| Amazon RDS Oracle, Oracle Autonomous Database                                           | Oracle                     |
| Amazon DocumentDB, Azure Cosmos DB (MongoDB API), MongoDB Atlas, FerretDB               | MongoDB                    |
| Amazon ElastiCache / MemoryDB, Azure Cache for Redis, Valkey, Dragonfly, KeyDB, Upstash | Redis                      |
| Amazon OpenSearch Service, Elastic Cloud                                                | Elasticsearch / OpenSearch |
| DynamoDB Local                                                                          | DynamoDB                   |
| Google Cloud SQL (Postgres / MySQL), AlloyDB                                            | PostgreSQL / MySQL         |
| Azure Database for PostgreSQL / MySQL                                                   | PostgreSQL / MySQL         |
| Supabase, Neon, Timescale, YugabyteDB, CockroachDB Cloud                                | PostgreSQL                 |
| PlanetScale, TiDB, SingleStore                                                          | MySQL                      |
| ScyllaDB                                                                                | Cassandra                  |
| ClickHouse Cloud                                                                        | ClickHouse                 |
| SurrealDB Cloud                                                                         | SurrealDB                  |

> Redshift and Cosmos DB implement only part of their upstream protocol. Queries work, but some schema-browser features may be limited.
