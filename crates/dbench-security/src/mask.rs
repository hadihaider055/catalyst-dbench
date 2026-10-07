//! Column masking and PII protection.
//!
//! Query results containing sensitive columns (PII, credentials, financial data)
//! can be masked before being sent to the UI. This is configured per-connection
//! in the connection config.
//!
//! Masking happens in the engine layer BEFORE data reaches the Tauri IPC boundary.

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{crypto::sha256_hex, Result, SecurityError};

/// How a sensitive column value should be masked.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
#[derive(Default)]
pub enum MaskMode {
    /// Replace the entire value with asterisks: `"***"`.
    #[default]
    FullRedact,
    /// Show only the last N characters, mask the rest: `"****1234"`.
    ShowLast { chars: usize },
    /// Show only the first N characters, mask the rest: `"Joh*****"`.
    ShowFirst { chars: usize },
    /// Replace with SHA-256 hash (allows correlation without exposing the value).
    Hash,
    /// Replace with a constant placeholder string.
    Placeholder { text: String },
}

/// A masking rule for a column name pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMask {
    /// Regex pattern matching column names (case-insensitive).
    pub column_pattern: String,
    /// How to mask matching values.
    pub mode: MaskMode,
}

impl ColumnMask {
    /// Create a mask rule that fully redacts columns matching a pattern.
    #[must_use]
    pub fn redact(pattern: impl Into<String>) -> Self {
        Self {
            column_pattern: pattern.into(),
            mode: MaskMode::FullRedact,
        }
    }

    /// Create a mask rule showing only the last N characters.
    #[must_use]
    pub fn show_last(pattern: impl Into<String>, chars: usize) -> Self {
        Self {
            column_pattern: pattern.into(),
            mode: MaskMode::ShowLast { chars },
        }
    }
}

/// A compiled, ready-to-use masking engine built from a list of [`ColumnMask`] rules.
pub struct MaskEngine {
    rules: Vec<(Regex, MaskMode)>,
}

impl MaskEngine {
    /// Compile a set of masking rules into a `MaskEngine`.
    ///
    /// # Errors
    /// Fails if any column pattern is not a valid regex.
    pub fn new(masks: &[ColumnMask]) -> Result<Self> {
        let rules = masks
            .iter()
            .map(|m| {
                // Case-insensitive match
                let pattern = format!("(?i)^{}$", m.column_pattern);
                let re = Regex::new(&pattern).map_err(|e| {
                    SecurityError::Validation(format!(
                        "Invalid column mask pattern '{}': {}",
                        m.column_pattern, e
                    ))
                })?;
                Ok((re, m.mode.clone()))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { rules })
    }

    /// Check whether a column name matches any masking rule.
    #[must_use]
    pub fn should_mask(&self, column_name: &str) -> bool {
        self.rules.iter().any(|(re, _)| re.is_match(column_name))
    }

    /// Apply masking to a cell value given its column name.
    ///
    /// Returns the masked string, or the original if no rule matches.
    #[must_use]
    pub fn mask_value(&self, column_name: &str, value: &str) -> String {
        for (re, mode) in &self.rules {
            if re.is_match(column_name) {
                return apply_mask(value, mode);
            }
        }
        value.to_owned()
    }

    /// Returns the names of columns that will be masked, given a list of column names.
    #[must_use]
    pub fn masked_columns<'a>(&self, columns: &[&'a str]) -> Vec<&'a str> {
        columns
            .iter()
            .copied()
            .filter(|c| self.should_mask(c))
            .collect()
    }
}

fn apply_mask(value: &str, mode: &MaskMode) -> String {
    match mode {
        MaskMode::FullRedact => "***".to_owned(),

        MaskMode::ShowLast { chars } => {
            let n = value.len();
            if n <= *chars {
                return value.to_owned(); // too short to mask
            }
            let visible = &value[n - chars..];
            format!("{}{}", "*".repeat(n - chars), visible)
        }

        MaskMode::ShowFirst { chars } => {
            let n = value.len();
            if n <= *chars {
                return value.to_owned();
            }
            let visible = &value[..*chars];
            format!("{}{}", visible, "*".repeat(n - chars))
        }

        MaskMode::Hash => format!("sha256:{}", sha256_hex(value.as_bytes())),

        MaskMode::Placeholder { text } => text.clone(),
    }
}

/// Default masking rules for common sensitive column patterns.
///
/// These are applied when a connection is marked as having PII protection enabled
/// but no explicit column masks are configured.
#[must_use]
pub fn default_sensitive_column_masks() -> Vec<ColumnMask> {
    vec![
        ColumnMask::redact("password"),
        ColumnMask::redact("password_hash"),
        ColumnMask::redact("secret"),
        ColumnMask::redact("token"),
        ColumnMask::redact("api_key"),
        ColumnMask::redact("access_token"),
        ColumnMask::redact("refresh_token"),
        ColumnMask::redact("private_key"),
        ColumnMask::redact("credit_card.*"),
        ColumnMask::redact("card_number"),
        ColumnMask::redact("cvv"),
        ColumnMask::show_last("ssn", 4),
        ColumnMask::show_last("social_security.*", 4),
        ColumnMask::show_last("phone", 4),
        ColumnMask {
            column_pattern: "email".into(),
            mode: MaskMode::Placeholder {
                text: "***@***.***".into(),
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_redact() {
        let engine = MaskEngine::new(&[ColumnMask::redact("password")]).unwrap();
        assert_eq!(engine.mask_value("password", "supersecret"), "***");
        assert_eq!(engine.mask_value("username", "alice"), "alice"); // not masked
    }

    #[test]
    fn show_last() {
        let engine = MaskEngine::new(&[ColumnMask::show_last("ssn", 4)]).unwrap();
        assert_eq!(engine.mask_value("ssn", "123456789"), "*****6789");
    }

    #[test]
    fn hash_mode() {
        let masks = vec![ColumnMask {
            column_pattern: "email".into(),
            mode: MaskMode::Hash,
        }];
        let engine = MaskEngine::new(&masks).unwrap();
        let result = engine.mask_value("email", "alice@example.com");
        assert!(result.starts_with("sha256:"));
    }

    #[test]
    fn case_insensitive_match() {
        let engine = MaskEngine::new(&[ColumnMask::redact("password")]).unwrap();
        assert_eq!(engine.mask_value("PASSWORD", "secret"), "***");
        assert_eq!(engine.mask_value("Password", "secret"), "***");
    }

    #[test]
    fn wildcard_pattern() {
        let engine = MaskEngine::new(&[ColumnMask::redact("credit_card.*")]).unwrap();
        assert_eq!(
            engine.mask_value("credit_card_number", "4111111111111111"),
            "***"
        );
        assert_eq!(engine.mask_value("credit_card_cvv", "123"), "***");
    }

    #[test]
    fn default_masks_compile() {
        let masks = default_sensitive_column_masks();
        assert!(MaskEngine::new(&masks).is_ok());
    }
}
