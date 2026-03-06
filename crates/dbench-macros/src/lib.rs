//! # dbench-macros
//!
//! Procedural macros that eliminate boilerplate for database driver authors.
//!
//! ## `#[derive(ConnectionConfig)]`
//!
//! Automatically implements [`dbench_core::driver::ConnectionConfig`] for a struct.
//!
//! Fields marked with `#[config(secret)]` are:
//! - Excluded from `display_string()` output (no accidental credential logging)
//! - Typed as `secrecy::Secret<String>` at compile time
//! - Loaded from the OS keychain, not from the config file on disk
//!
//! ### Example
//!
//! ```rust,ignore
//! use dbench_macros::ConnectionConfig;
//!
//! #[derive(ConnectionConfig, Debug, Clone, serde::Serialize, serde::Deserialize)]
//! pub struct PostgresConfig {
//!     pub host: String,
//!     pub port: u16,
//!     pub database: String,
//!     pub username: String,
//!     #[config(secret)]
//!     pub password: Option<String>,
//! }
//!
//! // Generated:
//! // - impl ConnectionConfig for PostgresConfig
//! // - fn validate(&self) -> Result<()>  — checks required fields
//! // - fn display_string(&self) -> String — omits password
//! ```

use darling::{ast, FromDeriveInput, FromField};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident, Type};

// ---------------------------------------------------------------------------
// ConnectionConfig derive
// ---------------------------------------------------------------------------

/// Field-level options for `#[derive(ConnectionConfig)]`.
#[derive(Debug, FromField)]
#[darling(attributes(config))]
struct ConfigField {
    ident: Option<Ident>,
    ty: Type,
    /// Mark this field as a secret — excluded from display, loaded from keychain.
    #[darling(default)]
    secret: bool,
    /// Mark this field as required (non-empty string / non-zero number).
    #[darling(default)]
    required: bool,
}

/// Struct-level options for `#[derive(ConnectionConfig)]`.
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(config), supports(struct_named))]
struct ConnectionConfigInput {
    ident: Ident,
    data: ast::Data<(), ConfigField>,
}

/// Derive macro that implements `dbench_core::driver::ConnectionConfig`.
///
/// See the crate-level documentation for usage examples.
#[proc_macro_derive(ConnectionConfig, attributes(config))]
pub fn derive_connection_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let parsed = match ConnectionConfigInput::from_derive_input(&input) {
        Ok(v) => v,
        Err(e) => return e.write_errors().into(),
    };

    let struct_name = &parsed.ident;
    let fields = match &parsed.data {
        ast::Data::Struct(fields) => &fields.fields,
        _ => unreachable!("only named structs are supported"),
    };

    // Build the validate() method body.
    // For each required field, emit a check.
    let validation_checks = fields.iter().filter_map(|f| {
        if !f.required {
            return None;
        }
        let field_name = f.ident.as_ref()?;
        let field_name_str = field_name.to_string();
        Some(quote! {
            if self.#field_name.is_empty() {
                return Err(dbench_core::CatalystError::Config(
                    format!("Field '{}' is required and cannot be empty", #field_name_str)
                ));
            }
        })
    });

    // Build the display_string() method body.
    // Include all non-secret fields; replace secrets with "[REDACTED]".
    let display_parts = fields.iter().filter_map(|f| {
        let field_name = f.ident.as_ref()?;
        let field_name_str = field_name.to_string();
        if f.secret {
            Some(quote! {
                parts.push(format!("{}=[REDACTED]", #field_name_str));
            })
        } else {
            Some(quote! {
                parts.push(format!("{}={:?}", #field_name_str, self.#field_name));
            })
        }
    });

    let expanded = quote! {
        impl dbench_core::driver::ConnectionConfig for #struct_name {
            fn validate(&self) -> dbench_core::Result<()> {
                #(#validation_checks)*
                Ok(())
            }

            fn display_string(&self) -> String {
                let mut parts: Vec<String> = Vec::new();
                #(#display_parts)*
                format!("{}({})", stringify!(#struct_name), parts.join(", "))
            }
        }
    };

    TokenStream::from(expanded)
}

// ---------------------------------------------------------------------------
// SchemaMapper derive (stub — to be implemented in Phase 2)
// ---------------------------------------------------------------------------

/// Derive macro that generates mapping from a driver-native schema type
/// to `dbench_core::schema::DatabaseSchema`.
///
/// Not yet fully implemented. Stub provided so the crate compiles.
#[proc_macro_derive(SchemaMapper, attributes(schema))]
pub fn derive_schema_mapper(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    // For now just emit an empty impl as a placeholder.
    let expanded = quote! {
        // SchemaMapper for #struct_name — full implementation in Phase 2
    };

    TokenStream::from(expanded)
}
