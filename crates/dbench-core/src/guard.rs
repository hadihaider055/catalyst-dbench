//! Read-only enforcement and identifier/literal quoting helpers shared by drivers.
//!
//! Drivers should prefer server-side enforcement (e.g. `SET TRANSACTION READ ONLY`,
//! ClickHouse `readonly=1`). This guard is the client-side layer for engines that
//! have no such switch, and defence in depth for those that do.

/// Keywords that modify data or schema in most SQL dialects.
pub const SQL_WRITE_KEYWORDS: &[&str] = &[
    "INSERT",
    "UPDATE",
    "DELETE",
    "MERGE",
    "UPSERT",
    "REPLACE",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "RENAME",
    "GRANT",
    "REVOKE",
    "DENY",
    "COMMIT",
    "EXEC",
    "EXECUTE",
    "CALL",
    "BEGIN",
    "DECLARE",
    "INTO",
    "BACKUP",
    "RESTORE",
    "BULK",
    "OPTIMIZE",
    "COPY",
    "LOCK",
    "DEFINE",
    "REMOVE",
    "RELATE",
    "KILL",
    "REBUILD",
    "SET",
    // Session switches that could turn a server-side READ ONLY back off
    // (`RESET ALL`, `SELECT set_config('default_transaction_read_only', 'off', …)`,
    // `START TRANSACTION READ WRITE`).
    "RESET",
    "DISCARD",
    "SET_CONFIG",
    "TRANSACTION",
    // Admin statements with side effects (SQL Server, Oracle).
    "DBCC",
    "SHUTDOWN",
    "RECONFIGURE",
    "DISABLE",
    "ENABLE",
    "PURGE",
    "FLASHBACK",
];

/// Remove `-- …` line comments, `/* … */` block comments, and the contents
/// of quoted strings/identifiers so keyword scanning can't be fooled by them.
#[must_use]
pub fn strip_comments_and_strings(sql: &str) -> String {
    let b = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i += 2;
                out.push(' ');
            }
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() {
                    if b[i] == q {
                        // Doubled quote = escaped quote, stay inside the literal.
                        if b.get(i + 1) == Some(&q) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
                out.push_str(" '' ");
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

/// `true` if `sql` contains any of `keywords` as a whole word, ignoring comments,
/// string literals and case. `#` comments and backslash escapes are deliberately
/// *not* honoured: dialect-specific, and honouring them could hide a keyword. Conservative: may reject a read that merely uses an
/// unquoted column named like a keyword — the safe direction for read-only mode.
#[must_use]
pub fn contains_keyword(sql: &str, keywords: &[&str]) -> bool {
    strip_comments_and_strings(sql)
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| keywords.iter().any(|k| k.eq_ignore_ascii_case(w)))
}

/// `true` if the statement would write, per [`SQL_WRITE_KEYWORDS`].
#[must_use]
pub fn is_sql_write(sql: &str) -> bool {
    contains_keyword(sql, SQL_WRITE_KEYWORDS)
}

/// Quote a string as a SQL literal: `O'Brien` → `'O''Brien'`.
#[must_use]
pub fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Quote an identifier with the given quote char, doubling any embedded quotes:
/// `quote_ident("a\"b", '"')` → `"a""b"`.
#[must_use]
pub fn quote_ident(s: &str, q: char) -> String {
    let close = if q == '[' { ']' } else { q };
    let escaped = s.replace(close, &format!("{close}{close}"));
    format!("{q}{escaped}{close}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_sees_through_comments_and_strings() {
        assert!(is_sql_write("-- harmless\nDROP TABLE t"));
        assert!(is_sql_write("/* x */ delete from t"));
        assert!(is_sql_write("SELECT 1; UPDATE t SET a = 1"));
        assert!(is_sql_write(
            "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"
        ));
        assert!(!is_sql_write("SELECT 'DROP TABLE t' AS s FROM users"));
        assert!(!is_sql_write("SELECT \"delete\" FROM t -- update"));
        assert!(!is_sql_write("SELECT 'it''s; drop' FROM t"));
        assert!(!is_sql_write("SELECT created_at, updated_by FROM t"));
        // Dialect tricks must not hide a write.
        assert!(is_sql_write("SELECT 1 # 2; DROP TABLE t"));
        assert!(is_sql_write("SELECT 'x\\'; DROP TABLE t; --'"));
        // Ways to switch a server-side read-only session back to read-write.
        assert!(is_sql_write("RESET ALL"));
        assert!(is_sql_write("DISCARD ALL"));
        assert!(is_sql_write("START TRANSACTION READ WRITE"));
        assert!(is_sql_write(
            "SELECT set_config('default_transaction_read_only', 'off', false)"
        ));
        assert!(is_sql_write("DBCC SHRINKDATABASE(0)"));
    }

    #[test]
    fn quoting_escapes_embedded_quotes() {
        assert_eq!(
            quote_literal("x'; DROP TABLE y;--"),
            "'x''; DROP TABLE y;--'"
        );
        assert_eq!(quote_ident("a\"b", '"'), "\"a\"\"b\"");
        assert_eq!(quote_ident("a`b", '`'), "`a``b`");
        assert_eq!(quote_ident("a]b", '['), "[a]]b]");
    }
}
