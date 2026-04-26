//! Procedural macros for the `babar` PostgreSQL driver.
//!
//! The main entry points are [`sql!`](macro@sql) and the `#[derive(Codec)]`
//! derive, both re-exported from the `babar` crate.

use proc_macro::TokenStream;
use std::collections::{HashMap, HashSet};

use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, ExprMacro, Field, Fields, Generics,
    Ident, LitStr, Result, Token,
};

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

/// Derive a `CODEC` associated constant for a named struct.
///
/// Each field must declare its codec with `#[pg(codec = "...")]`. The string
/// is parsed as a Rust expression in a scope that brings `babar::codec::*`
/// into scope, so values like `"int4"`, `"nullable(text)"`, or
/// `"typed_json::<MyType>()"` all work.
#[proc_macro_derive(Codec, attributes(pg))]
pub fn derive_codec(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match compile_codec_derive(&input) {
        Ok(tokens) => tokens.into(),
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

fn compile_codec_derive(input: &DeriveInput) -> Result<proc_macro2::TokenStream> {
    reject_generics(&input.generics)?;

    let ident = &input.ident;
    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "Codec can only be derived for structs",
            ))
        }
    };
    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        _ => {
            return Err(syn::Error::new_spanned(
                data.fields.clone(),
                "Codec requires a struct with named fields",
            ))
        }
    };

    let codec_ident = format_ident!("__Babar{}Codec", ident);
    let assert_ident = format_ident!("__babar_assert_field_codecs_for_{}", ident);
    let mut field_idents = Vec::new();
    let mut field_types = Vec::new();
    let mut codec_exprs = Vec::new();
    let mut decoded_idents = Vec::new();
    let mut assert_blocks = Vec::new();

    for (index, field) in fields.iter().enumerate() {
        let field_ident = field.ident.clone().expect("named field");
        let field_ty = field.ty.clone();
        let codec_expr = parse_codec_attr(field)?;
        let decoded_ident = format_ident!("__babar_field_{index}");

        let assert_block = quote! {
            {
                fn assert_codec<C>(_: &C)
                where
                    C: ::babar::codec::Encoder<#field_ty> + ::babar::codec::Decoder<#field_ty>,
                {}
                assert_codec(&{ use ::babar::codec::*; #codec_expr });
            }
        };

        field_idents.push(field_ident);
        field_types.push(field_ty);
        codec_exprs.push(codec_expr);
        decoded_idents.push(decoded_ident);
        assert_blocks.push(assert_block);
    }

    let tuple_type = quote! { (#(#field_types,)*) };
    let tuple_codec = quote! { (#({ use ::babar::codec::*; #codec_exprs },)*) };
    let encode_fields =
        field_idents
            .iter()
            .zip(codec_exprs.iter())
            .map(|(field_ident, codec_expr)| {
                quote! {
                    ::babar::codec::Encoder::encode(
                        &{ use ::babar::codec::*; #codec_expr },
                        &value.#field_ident,
                        params,
                    )?;
                }
            });

    let expanded = quote! {
        #[doc(hidden)]
        #[derive(Clone, Copy, Debug)]
        pub struct #codec_ident;

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #assert_ident() {
            #(#assert_blocks)*
        }

        impl #ident {
            /// Codec generated by `#[derive(Codec)]` for this struct.
            pub const CODEC: #codec_ident = #codec_ident;
        }

        impl ::babar::codec::Encoder<#ident> for #codec_ident {
            fn encode(
                &self,
                value: &#ident,
                params: &mut ::std::vec::Vec<::core::option::Option<::std::vec::Vec<u8>>>,
            ) -> ::babar::Result<()> {
                #assert_ident();
                #(#encode_fields)*
                ::core::result::Result::Ok(())
            }

            fn oids(&self) -> &'static [::babar::types::Oid] {
                #assert_ident();
                let codec = #tuple_codec;
                <_ as ::babar::codec::Encoder<#tuple_type>>::oids(&codec)
            }

            fn format_codes(&self) -> &'static [i16] {
                #assert_ident();
                let codec = #tuple_codec;
                <_ as ::babar::codec::Encoder<#tuple_type>>::format_codes(&codec)
            }
        }

        impl ::babar::codec::Decoder<#ident> for #codec_ident {
            fn decode(&self, columns: &[::core::option::Option<::babar::__private::Bytes>]) -> ::babar::Result<#ident> {
                #assert_ident();
                let codec = #tuple_codec;
                let (#(#decoded_idents,)*) = <_ as ::babar::codec::Decoder<#tuple_type>>::decode(&codec, columns)?;
                ::core::result::Result::Ok(#ident { #(#field_idents: #decoded_idents,)* })
            }

            fn n_columns(&self) -> usize {
                #assert_ident();
                let codec = #tuple_codec;
                <_ as ::babar::codec::Decoder<#tuple_type>>::n_columns(&codec)
            }

            fn oids(&self) -> &'static [::babar::types::Oid] {
                #assert_ident();
                let codec = #tuple_codec;
                <_ as ::babar::codec::Decoder<#tuple_type>>::oids(&codec)
            }

            fn format_codes(&self) -> &'static [i16] {
                #assert_ident();
                let codec = #tuple_codec;
                <_ as ::babar::codec::Decoder<#tuple_type>>::format_codes(&codec)
            }
        }
    };

    Ok(expanded)
}

fn reject_generics(generics: &Generics) -> Result<()> {
    if generics.params.is_empty() {
        return Ok(());
    }
    Err(syn::Error::new_spanned(
        generics,
        "Codec derive does not support generic structs yet",
    ))
}

fn parse_codec_attr(field: &Field) -> Result<Expr> {
    let mut codec = None;
    for attr in &field.attrs {
        if !attr.path().is_ident("pg") {
            continue;
        }
        parse_pg_attr(attr, &mut codec)?;
    }

    codec.ok_or_else(|| syn::Error::new_spanned(field, "missing #[pg(codec = \"...\")] attribute"))
}

fn parse_pg_attr(attr: &Attribute, codec: &mut Option<Expr>) -> Result<()> {
    attr.parse_nested_meta(|meta| {
        if !meta.path.is_ident("codec") {
            return Err(meta.error("unsupported #[pg(...)] attribute; expected codec = \"...\""));
        }
        if codec.is_some() {
            return Err(meta.error("duplicate codec attribute"));
        }
        let lit: LitStr = meta.value()?.parse()?;
        *codec = Some(lit.parse()?);
        Ok(())
    })
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
