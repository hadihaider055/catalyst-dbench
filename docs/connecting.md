# Connecting

Most databases use the usual Host / Port / Database / Username / Password fields. For the rest, the dialog relabels the fields as shown below:

| Database                       | Host                                                                   | Port                     | Database field                          | Username / Password                                                               |
| ------------------------------ | ---------------------------------------------------------------------- | ------------------------ | --------------------------------------- | --------------------------------------------------------------------------------- |
| **SQL Server / Azure SQL**     | Server name                                                            | 1433                     | Database (optional)                     | SQL login                                                                         |
| **Oracle**                     | Hostname, or a full `(DESCRIPTION=…)` TNS descriptor                   | 1521 (TCPS: 1522 / 2484) | **Service name**, e.g. `FREEPDB1`       | Database user                                                                     |
| **DynamoDB**                   | **Region** (`us-east-1`), or an endpoint URL (`http://localhost:8000`) | n/a                      | Region (only when Host is an endpoint)  | Access key ID / secret. Leave both empty to use your AWS profile, env vars or SSO |
| **Elasticsearch / OpenSearch** | Hostname                                                               | 9200 (cloud: 443)        | Index pattern, e.g. `logs-*` (optional) | Basic auth, **or** leave username empty and paste an API key as the password      |
| **SurrealDB**                  | Hostname                                                               | 8000                     | `namespace/database`                    | Root, namespace or database user                                                  |
| **Redis**                      | Hostname                                                               | 6379                     | DB index                                | ACL user (optional) / password                                                    |

**Aurora, RDS and other AWS databases.** TLS certificates are verified, and AWS signs them with its own certificate authority, which your OS doesn't trust by default. Download the [AWS RDS CA bundle](https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem) and enter its path in **CA certificate** (shown when TLS is on). For **Aurora DSQL**, generate an IAM auth token and use it as the password.

**Query examples for the newer drivers:**

```sql
-- SQL Server: T-SQL
SELECT TOP 10 name, create_date FROM sys.tables ORDER BY create_date DESC;

-- Oracle: no trailing semicolon needed; PL/SQL blocks keep their END;
SELECT table_name FROM user_tables FETCH FIRST 10 ROWS ONLY

-- DynamoDB: PartiQL (quote table names; results are capped at 5,000 items)
SELECT * FROM "Orders" WHERE customer_id = 'c-42'

-- SurrealDB: SurrealQL
SELECT * FROM person WHERE age > 30 LIMIT 20;
```

```
# Elasticsearch / OpenSearch: Dev Tools console syntax (or plain SQL)
GET /logs-*/_search
{
  "size": 20,
  "query": { "match": { "level": "error" } }
}
```
