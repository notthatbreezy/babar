//! Procedural macros for the `babar` PostgreSQL driver.
//!
//! The main entry point is the [`sql!`](macro@sql) macro, re-exported as
//! `babar::sql!` for end users.

use proc_macro::TokenStream;
use std::collections::{HashMap, HashSet};

use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, ExprMacro, Ident, LitStr, Result, Token};

struct SqlInput {
    sql: LitStr,
    bindings: Vec<Binding>,
}

struct Binding {
    name: Ident,
    expr: Expr,
}

struct CompiledSql {
    sql: String,
    codecs: Vec<Expr>,
}

impl Parse for SqlInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let sql = input.parse()?;
        let mut bindings = Vec::new();

        if input.is_empty() {
            return Ok(Self { sql, bindings });
        }

        input.parse::<Token![,]>()?;
        while !input.is_empty() {
            let name = input.parse()?;
            input.parse::<Token![=]>()?;
            let expr = input.parse()?;
            bindings.push(Binding { name, expr });
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        Ok(Self { sql, bindings })
    }
}

/// Build a `babar::query::Fragment` from SQL that uses named placeholders.
///
/// The accepted placeholder syntax is `$name`, chosen for babar v0.1. Bindings
/// are supplied as `name = codec` pairs after the SQL string:
///
/// - every placeholder must have a matching binding,
/// - every binding must be used,
/// - repeating the same placeholder reuses one parameter slot, and
/// - nested `sql!(...)` calls flatten into one fragment in left-to-right order.
///
/// The macro does not validate SQL against a live database, infer codecs, or
/// interpolate identifiers. It only rewrites placeholders, builds the fragment
/// encoder, and captures the callsite for origin tracking.
#[proc_macro]
pub fn sql(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SqlInput);
    match compile_input(&input) {
        Ok(compiled) => {
            let sql = LitStr::new(&compiled.sql, input.sql.span());
            let codecs = compiled.codecs;
            let n_params = codecs.len();
            quote! {{
                ::babar::query::Fragment::__from_parts(
                    #sql,
                    (#(#codecs,)*),
                    #n_params,
                    ::core::option::Option::Some(::babar::query::Origin::new(
                        file!(),
                        line!(),
                        column!(),
                    )),
                )
            }}
            .into()
        }
        Err(err) => err.into_compile_error().into(),
    }
}

fn compile_input(input: &SqlInput) -> Result<CompiledSql> {
    let mut seen = HashSet::new();
    let mut bindings = HashMap::new();
    for binding in &input.bindings {
        let name = binding.name.to_string();
        if !seen.insert(name.clone()) {
            return Err(syn::Error::new(
                binding.name.span(),
                format!("duplicate sql! binding `{name}`"),
            ));
        }
        bindings.insert(name, binding);
    }

    let mut sql = String::with_capacity(input.sql.value().len());
    let mut codecs = Vec::new();
    let mut slots = HashMap::<String, usize>::new();
    let mut used = HashSet::<String>::new();
    let source = input.sql.value();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && is_ident_start(chars[i + 1]) {
            let start = i + 1;
            i += 2;
            while i < chars.len() && is_ident_continue(chars[i]) {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            let Some(binding) = bindings.get(&name) else {
                return Err(syn::Error::new(
                    input.sql.span(),
                    format!("sql! placeholder `${name}` has no binding"),
                ));
            };
            used.insert(name.clone());

            if let Some(nested) = nested_sql(&binding.expr)? {
                let compiled = compile_input(&nested)?;
                renumber_into(&compiled.sql, codecs.len(), &mut sql);
                codecs.extend(compiled.codecs);
                continue;
            }

            let slot = *slots.entry(name).or_insert_with(|| {
                codecs.push(binding.expr.clone());
                codecs.len()
            });
            sql.push('$');
            sql.push_str(&slot.to_string());
            continue;
        }

        sql.push(chars[i]);
        i += 1;
    }

    for binding in &input.bindings {
        let name = binding.name.to_string();
        if !used.contains(&name) {
            return Err(syn::Error::new(
                binding.name.span(),
                format!("sql! binding `{name}` is unused"),
            ));
        }
    }

    Ok(CompiledSql { sql, codecs })
}

fn nested_sql(expr: &Expr) -> Result<Option<SqlInput>> {
    let Expr::Macro(ExprMacro { mac, .. }) = expr else {
        return Ok(None);
    };
    if !path_ends_with_sql(&mac.path) {
        return Ok(None);
    }
    syn::parse2::<SqlInput>(mac.tokens.clone()).map(Some)
}

fn path_ends_with_sql(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "sql")
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn renumber_into(sql: &str, offset: usize, out: &mut String) {
    if offset == 0 {
        out.push_str(sql);
        return;
    }

    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let slot = std::str::from_utf8(&bytes[i + 1..j])
                .expect("nested sql placeholder digits")
                .parse::<usize>()
                .expect("nested sql placeholder parses")
                + offset;
            out.push('$');
            out.push_str(&slot.to_string());
            i = j;
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }
}
