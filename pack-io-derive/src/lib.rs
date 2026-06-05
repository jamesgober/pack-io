//! Procedural macros for [`pack-io`].
//!
//! Three derives are provided:
//!
//! - `#[derive(Serialize)]` — implements `pack_io::Serialize`.
//! - `#[derive(Deserialize)]` — implements `pack_io::Deserialize` (owning).
//! - `#[derive(DeserializeView)]` — implements `pack_io::DeserializeView`
//!   for types whose fields borrow from the input buffer (`&'a str`,
//!   `&'a [u8]`, primitives).
//!
//! All three work on structs (named, tuple, unit) and enums (any variant
//! shape). Field order in the source code is the encoded byte order; the
//! wire format adds a `varint(variant_index)` prefix for enums.
//!
//! This crate is not intended to be used directly — depend on
//! [`pack-io`] with the `derive` feature instead, which re-exports the
//! macros at `pack_io::{Serialize, Deserialize, DeserializeView}`.
//!
//! [`pack-io`]: https://crates.io/crates/pack-io

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DataEnum, DataStruct, DeriveInput, Field, Fields, GenericParam, Generics,
    Ident, Index, Lifetime, LifetimeParam, parse_macro_input, parse_quote,
};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Derive `pack_io::Serialize` for a struct or enum.
///
/// Fields are encoded in their declaration order. For enums, a
/// `varint(variant_index)` is emitted first, followed by the variant's
/// fields.
///
/// Each field type must already implement `pack_io::Serialize`.
#[proc_macro_derive(Serialize, attributes(pack_io))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_serialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive `pack_io::Deserialize` for a struct or enum (owning decode).
///
/// Each field type must already implement `pack_io::Deserialize`.
#[proc_macro_derive(Deserialize, attributes(pack_io))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_deserialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive `pack_io::DeserializeView` for a struct or enum (zero-copy decode).
///
/// The type MUST have exactly one lifetime parameter — used as the borrow
/// of the underlying input buffer. Field types must implement
/// `pack_io::DeserializeView<'that_lifetime>`. The built-in impls for
/// primitives, `&'a str`, `&'a [u8]`, and the standard `Option` / `Result`
/// / tuple / array types cover the common cases.
///
/// Enums use the same `varint(variant_index) ++ fields` wire shape as
/// `#[derive(Deserialize)]`; the only difference is each variant's fields
/// are decoded via `DeserializeView` so borrow-shaped fields land as
/// `&'a str` / `&'a [u8]` rather than `String` / `Vec<u8>`.
#[proc_macro_derive(DeserializeView, attributes(pack_io))]
pub fn derive_deserialize_view(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_deserialize_view(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

// ---------------------------------------------------------------------------
// Serialize
// ---------------------------------------------------------------------------

fn expand_serialize(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let generics = add_trait_bound(&input.generics, parse_quote!(::pack_io::Serialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let type_version = parse_type_attrs(&input.attrs)?;

    let body = match &input.data {
        Data::Struct(data) => serialize_struct_body(name, data, type_version)?,
        Data::Enum(data) => {
            if type_version.is_some() {
                return Err(syn::Error::new_spanned(
                    name,
                    "pack-io: `#[pack_io(version = N)]` is not supported on enums in this \
                     release; wrap the enum in a versioned newtype struct instead",
                ));
            }
            serialize_enum_body(name, data)?
        }
        Data::Union(u) => {
            return Err(syn::Error::new(
                u.union_token.span(),
                "pack-io: `union` is not supported (no defined wire format)",
            ));
        }
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::pack_io::Serialize for #name #ty_generics #where_clause {
            #[inline]
            fn serialize<__E: ::pack_io::Encode + ?Sized>(
                &self,
                __encoder: &mut __E,
            ) -> ::pack_io::Result<()> {
                #body
                ::core::result::Result::Ok(())
            }
        }
    })
}

fn serialize_struct_body(
    name: &Ident,
    data: &DataStruct,
    type_version: Option<u32>,
) -> syn::Result<TokenStream2> {
    // Pair each field accessor with its schema attrs so we can filter / sort.
    let fields_meta = collect_field_meta(&data.fields, quote!(self))?;

    // Reject schema attrs on a non-versioned struct — they have no meaning
    // without an enclosing version.
    if type_version.is_none() {
        for fm in &fields_meta {
            if fm.attrs.since.is_some() || fm.attrs.deprecated.is_some() {
                return Err(syn::Error::new_spanned(
                    name,
                    "pack-io: `#[pack_io(since = N)]` / `#[pack_io(deprecated = N)]` requires \
                     the struct itself to carry `#[pack_io(version = N)]`",
                ));
            }
        }
    }

    match type_version {
        None => {
            // Plain (v0.4) encoding: fields concatenated in declaration order.
            let writes = fields_meta.iter().map(|fm| {
                let acc = &fm.accessor;
                quote! { ::pack_io::Serialize::serialize(&#acc, __encoder)?; }
            });
            Ok(quote! { #(#writes)* })
        }
        Some(version) => {
            // Versioned encoding: varint(version) ++ varint(body_len) ++ body.
            // Body holds exactly the fields whose [since, deprecated) window
            // includes `version`.
            let writes = fields_meta
                .iter()
                .filter(|fm| fm.attrs.live_at(version))
                .map(|fm| {
                    let acc = &fm.accessor;
                    quote! { ::pack_io::Serialize::serialize(&#acc, &mut __body)?; }
                });
            Ok(quote! {
                __encoder.write_varint_u64(#version as u64)?;
                let mut __body = ::pack_io::Encoder::new();
                #(#writes)*
                let __body_bytes = ::pack_io::Encoder::into_inner(__body);
                __encoder.write_varint_u64(__body_bytes.len() as u64)?;
                __encoder.write_bytes(&__body_bytes)?;
            })
        }
    }
}

fn serialize_enum_body(name: &Ident, data: &DataEnum) -> syn::Result<TokenStream2> {
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "pack-io: empty enums cannot be serialised (no value to encode)",
        ));
    }

    let arms: Vec<TokenStream2> = data
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let index = u32::try_from(index).expect("u32 enum variants");
            let var_name = &variant.ident;
            let bindings = variant_bindings(&variant.fields);
            let pattern = match &variant.fields {
                Fields::Named(_) => quote!(Self::#var_name { #(#bindings),* }),
                Fields::Unnamed(_) => quote!(Self::#var_name(#(#bindings),*)),
                Fields::Unit => quote!(Self::#var_name),
            };
            let writes = bindings.iter().map(|b| {
                quote! { ::pack_io::Serialize::serialize(#b, __encoder)?; }
            });
            quote! {
                #pattern => {
                    __encoder.write_varint_u64(#index as u64)?;
                    #(#writes)*
                }
            }
        })
        .collect();

    Ok(quote! {
        match self {
            #(#arms)*
        }
    })
}

// ---------------------------------------------------------------------------
// Deserialize (owning)
// ---------------------------------------------------------------------------

fn expand_deserialize(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let generics = add_trait_bound(&input.generics, parse_quote!(::pack_io::Deserialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let type_version = parse_type_attrs(&input.attrs)?;

    let body = match &input.data {
        Data::Struct(data) => deserialize_struct_body(name, &data.fields, type_version)?,
        Data::Enum(data) => {
            if type_version.is_some() {
                return Err(syn::Error::new_spanned(
                    name,
                    "pack-io: `#[pack_io(version = N)]` is not supported on enums in this \
                     release; wrap the enum in a versioned newtype struct instead",
                ));
            }
            deserialize_enum_body(name, data)?
        }
        Data::Union(u) => {
            return Err(syn::Error::new(
                u.union_token.span(),
                "pack-io: `union` is not supported",
            ));
        }
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::pack_io::Deserialize for #name #ty_generics #where_clause {
            #[inline]
            fn deserialize<__D: ::pack_io::Decode + ?Sized>(
                __decoder: &mut __D,
            ) -> ::pack_io::Result<Self> {
                #body
            }
        }
    })
}

fn deserialize_struct_body(
    name: &Ident,
    fields: &Fields,
    type_version: Option<u32>,
) -> syn::Result<TokenStream2> {
    let fields_meta = collect_field_meta(fields, quote!(self))?;

    if type_version.is_none() {
        for fm in &fields_meta {
            if fm.attrs.since.is_some() || fm.attrs.deprecated.is_some() {
                return Err(syn::Error::new_spanned(
                    name,
                    "pack-io: `#[pack_io(since = N)]` / `#[pack_io(deprecated = N)]` requires \
                     the struct itself to carry `#[pack_io(version = N)]`",
                ));
            }
        }
    }

    match type_version {
        None => {
            // Plain (v0.4) decoding: each field's Deserialize impl in order.
            let constructor = construct_from_fields(quote!(#name), fields, |ty| {
                quote_spanned! { ty.span() =>
                    <#ty as ::pack_io::Deserialize>::deserialize(__decoder)?
                }
            });
            Ok(quote! { ::core::result::Result::Ok(#constructor) })
        }
        Some(_) => {
            // Versioned decoding: read varint(version), varint(body_len),
            // then run each field through a conditional based on its
            // [since, deprecated) window vs. the wire's version.
            //
            // Only fields whose [since, deprecated) window does NOT span
            // every possible version (i.e. `since > 1` or `deprecated` is
            // set) need a `Default` fallback. Fields that are always live
            // skip the conditional entirely — no `Default` bound required.
            let field_inits: Vec<TokenStream2> = fields_meta
                .iter()
                .enumerate()
                .map(|(i, fm)| {
                    let var = field_local_ident(fm, i);
                    let ty = &fm.ty;
                    let always_live =
                        fm.attrs.since.unwrap_or(1) == 1 && fm.attrs.deprecated.is_none();
                    if always_live {
                        quote! {
                            let #var: #ty =
                                <#ty as ::pack_io::Deserialize>::deserialize(&mut __body_dec)?;
                        }
                    } else {
                        let since = fm.attrs.since.unwrap_or(1);
                        let deprecated_check = match fm.attrs.deprecated {
                            Some(d) => quote! { __version < (#d as u32) },
                            None => quote! { true },
                        };
                        quote! {
                            let #var: #ty = if (#since as u32) <= __version
                                && #deprecated_check
                            {
                                <#ty as ::pack_io::Deserialize>::deserialize(&mut __body_dec)?
                            } else {
                                ::core::default::Default::default()
                            };
                        }
                    }
                })
                .collect();

            let constructor = match fields {
                Fields::Named(_) => {
                    let pairs = fields_meta.iter().enumerate().map(|(i, fm)| {
                        let name = fm.field_ident.as_ref().expect("named");
                        let var = field_local_ident(fm, i);
                        quote! { #name: #var }
                    });
                    quote! { #name { #(#pairs),* } }
                }
                Fields::Unnamed(_) => {
                    let positions = fields_meta
                        .iter()
                        .enumerate()
                        .map(|(i, fm)| field_local_ident(fm, i));
                    quote! { #name ( #(#positions),* ) }
                }
                Fields::Unit => quote! { #name },
            };

            Ok(quote! {
                let __version_u64 = __decoder.read_varint_u64()?;
                let __version = u32::try_from(__version_u64)
                    .map_err(|_| ::pack_io::SerialError::IntegerOutOfRange)?;
                let __body_len_u64 = __decoder.read_varint_u64()?;
                let __max = ::pack_io::Decode::max_alloc(__decoder) as u64;
                if __body_len_u64 > __max {
                    return ::core::result::Result::Err(::pack_io::SerialError::InvalidLength {
                        declared: __body_len_u64,
                        remaining: 0,
                    });
                }
                let __body_len = __body_len_u64 as usize;
                let mut __body_buf = ::std::vec![0u8; __body_len];
                __decoder.read_into(&mut __body_buf)?;
                let mut __body_dec = ::pack_io::Decoder::new(&__body_buf);
                #(#field_inits)*
                ::core::result::Result::Ok(#constructor)
            })
        }
    }
}

/// Emit the body of a `DeserializeView::deserialize_view` impl for an enum.
///
/// Mirrors [`deserialize_enum_body`] but routes every field decode through
/// `DeserializeView<'a>::deserialize_view` instead of
/// `Deserialize::deserialize`, so zero-copy borrows propagate into each
/// variant's fields.
fn deserialize_view_enum_body(
    name: &Ident,
    data: &DataEnum,
    lifetime: &Lifetime,
) -> syn::Result<TokenStream2> {
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "pack-io: empty enums cannot be deserialised",
        ));
    }

    let arms: Vec<TokenStream2> = data
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let index = u32::try_from(index).expect("u32 enum variants");
            let var_name = &variant.ident;
            let constructor =
                construct_from_fields(quote!(#name :: #var_name), &variant.fields, |ty| {
                    quote_spanned! { ty.span() =>
                        <#ty as ::pack_io::DeserializeView<#lifetime>>::deserialize_view(__decoder)?
                    }
                });
            quote! { #index => ::core::result::Result::Ok(#constructor), }
        })
        .collect();

    let enum_name = name.to_string();
    Ok(quote! {
        // Fully-qualified trait call — the `Decode` trait isn't in scope at
        // the user's call site, so a bare `__decoder.read_varint_u64()`
        // would fail method resolution.
        let __tag = ::pack_io::Decode::read_varint_u64(__decoder)?;
        let __idx = u32::try_from(__tag)
            .map_err(|_| ::pack_io::SerialError::UnknownVariant {
                kind: #enum_name,
                index: u64::MAX,
            })?;
        match __idx {
            #(#arms)*
            other => ::core::result::Result::Err(::pack_io::SerialError::UnknownVariant {
                kind: #enum_name,
                index: u64::from(other),
            }),
        }
    })
}

fn field_local_ident(fm: &FieldMeta<'_>, index: usize) -> Ident {
    match &fm.field_ident {
        Some(id) => Ident::new(&format!("__f_{}", id), id.span()),
        None => Ident::new(&format!("__f_{}", index), fm.ty.span()),
    }
}

fn deserialize_enum_body(name: &Ident, data: &DataEnum) -> syn::Result<TokenStream2> {
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "pack-io: empty enums cannot be deserialised",
        ));
    }

    let arms: Vec<TokenStream2> = data
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let index = u32::try_from(index).expect("u32 enum variants");
            let var_name = &variant.ident;
            let constructor =
                construct_from_fields(quote!(#name :: #var_name), &variant.fields, |ty| {
                    quote_spanned! { ty.span() =>
                        <#ty as ::pack_io::Deserialize>::deserialize(__decoder)?
                    }
                });
            quote! { #index => ::core::result::Result::Ok(#constructor), }
        })
        .collect();

    let enum_name = name.to_string();
    Ok(quote! {
        let __tag = __decoder.read_varint_u64()?;
        let __idx = u32::try_from(__tag)
            .map_err(|_| ::pack_io::SerialError::UnknownVariant {
                kind: #enum_name,
                index: u64::MAX,
            })?;
        match __idx {
            #(#arms)*
            other => ::core::result::Result::Err(::pack_io::SerialError::UnknownVariant {
                kind: #enum_name,
                index: u64::from(other),
            }),
        }
    })
}

// ---------------------------------------------------------------------------
// DeserializeView (zero-copy)
// ---------------------------------------------------------------------------

fn expand_deserialize_view(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    let lifetime = extract_single_lifetime(&input.generics).ok_or_else(|| {
        syn::Error::new_spanned(
            &input.generics,
            "pack-io: `DeserializeView` requires the type to have exactly one lifetime parameter \
             (used as the borrow of the input buffer)",
        )
    })?;

    let generics = add_trait_bound_with_lifetime(
        &input.generics,
        parse_quote!(::pack_io::DeserializeView<#lifetime>),
        &lifetime,
    );
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let body = match &input.data {
        Data::Struct(data) => {
            let constructor = construct_from_fields(quote!(#name), &data.fields, |ty| {
                quote_spanned! { ty.span() =>
                    <#ty as ::pack_io::DeserializeView<#lifetime>>::deserialize_view(__decoder)?
                }
            });
            quote! { ::core::result::Result::Ok(#constructor) }
        }
        Data::Enum(data) => deserialize_view_enum_body(name, data, &lifetime)?,
        Data::Union(u) => {
            return Err(syn::Error::new(
                u.union_token.span(),
                "pack-io: `union` is not supported",
            ));
        }
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::pack_io::DeserializeView<#lifetime> for #name #ty_generics #where_clause {
            #[inline]
            fn deserialize_view(
                __decoder: &mut ::pack_io::Decoder<#lifetime>,
            ) -> ::pack_io::Result<Self> {
                #body
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Schema attribute parsing
// ---------------------------------------------------------------------------

/// Schema attributes attached to a struct or field.
#[derive(Default, Clone, Copy)]
struct SchemaAttrs {
    /// Only meaningful on fields: the type version at which this field was
    /// added. Absent ⇒ `1` (always present).
    since: Option<u32>,
    /// Only meaningful on fields: the type version at which this field was
    /// removed. Absent ⇒ never deprecated.
    deprecated: Option<u32>,
}

impl SchemaAttrs {
    /// Is the field live at the given type version (`since <= v < deprecated`)?
    fn live_at(self, version: u32) -> bool {
        let since = self.since.unwrap_or(1);
        if version < since {
            return false;
        }
        match self.deprecated {
            Some(d) => version < d,
            None => true,
        }
    }
}

/// Per-field metadata used by the version-aware code generator.
struct FieldMeta<'a> {
    /// `self.<name>` (named) / `self.<index>` (tuple) / variant binding.
    accessor: TokenStream2,
    /// `Some(ident)` for named fields; `None` for tuple-struct positions.
    field_ident: Option<&'a Ident>,
    /// The field's type.
    ty: &'a syn::Type,
    /// Parsed `#[pack_io(...)]` attributes on the field.
    attrs: SchemaAttrs,
}

/// Parse `#[pack_io(...)]` attributes on a type. Only `version = N` is
/// accepted at the type level.
fn parse_type_attrs(attrs: &[Attribute]) -> syn::Result<Option<u32>> {
    let mut version: Option<u32> = None;
    for attr in attrs {
        if !attr.path().is_ident("pack_io") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("version") {
                let lit: syn::LitInt = meta.value()?.parse()?;
                let v: u32 = lit.base10_parse()?;
                if v == 0 {
                    return Err(meta.error("pack-io: schema versions start at 1, not 0"));
                }
                version = Some(v);
                Ok(())
            } else if meta.path.is_ident("since") || meta.path.is_ident("deprecated") {
                Err(meta.error(
                    "pack-io: `since` / `deprecated` are field-level attributes; only \
                     `version` is allowed on the type",
                ))
            } else {
                Err(meta.error("pack-io: unknown attribute (expected `version`)"))
            }
        })?;
    }
    Ok(version)
}

/// Parse `#[pack_io(...)]` attributes on a struct field.
fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<SchemaAttrs> {
    let mut out = SchemaAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("pack_io") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("since") {
                let lit: syn::LitInt = meta.value()?.parse()?;
                let v: u32 = lit.base10_parse()?;
                if v == 0 {
                    return Err(meta.error("pack-io: schema versions start at 1, not 0"));
                }
                out.since = Some(v);
                Ok(())
            } else if meta.path.is_ident("deprecated") {
                let lit: syn::LitInt = meta.value()?.parse()?;
                let v: u32 = lit.base10_parse()?;
                if v == 0 {
                    return Err(meta.error("pack-io: schema versions start at 1, not 0"));
                }
                out.deprecated = Some(v);
                Ok(())
            } else if meta.path.is_ident("version") {
                Err(meta.error(
                    "pack-io: `version` is a type-level attribute; use `since` / `deprecated` \
                     on individual fields",
                ))
            } else {
                Err(meta.error("pack-io: unknown attribute (expected `since` or `deprecated`)"))
            }
        })?;
    }
    if let (Some(since), Some(dep)) = (out.since, out.deprecated) {
        if dep <= since {
            return Err(syn::Error::new(
                attrs[0].span(),
                format!(
                    "pack-io: `deprecated = {dep}` must be strictly greater than \
                     `since = {since}` — a field cannot be removed before it is introduced",
                ),
            ));
        }
    }
    Ok(out)
}

/// Walk a struct's fields and collect their per-field metadata.
fn collect_field_meta(fields: &Fields, owner: TokenStream2) -> syn::Result<Vec<FieldMeta<'_>>> {
    let mut out = Vec::new();
    match fields {
        Fields::Named(named) => {
            for f in &named.named {
                let ident = f.ident.as_ref().expect("named fields have idents");
                out.push(FieldMeta {
                    accessor: quote!(#owner.#ident),
                    field_ident: Some(ident),
                    ty: &f.ty,
                    attrs: parse_field_attrs(&f.attrs)?,
                });
            }
        }
        Fields::Unnamed(unnamed) => {
            for (i, f) in unnamed.unnamed.iter().enumerate() {
                let idx = Index::from(i);
                out.push(FieldMeta {
                    accessor: quote!(#owner.#idx),
                    field_ident: None,
                    ty: &f.ty,
                    attrs: parse_field_attrs(&f.attrs)?,
                });
            }
        }
        Fields::Unit => {}
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Emit one identifier per field, for use in an enum-pattern binding.
fn variant_bindings(fields: &Fields) -> Vec<TokenStream2> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let name = f.ident.as_ref().expect("named fields have idents");
                quote!(#name)
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let id = Ident::new(&format!("__f{i}"), unnamed.unnamed[i].span());
                quote!(#id)
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

/// Build `Path { f1: …, f2: … }`, `Path(…, …)`, or `Path` from a `Fields`
/// description, using `gen_expr(field_type)` to produce each per-field
/// expression.
fn construct_from_fields<F>(path: TokenStream2, fields: &Fields, mut gen_expr: F) -> TokenStream2
where
    F: FnMut(&syn::Type) -> TokenStream2,
{
    match fields {
        Fields::Named(named) => {
            let pieces = named.named.iter().map(|f: &Field| {
                let name = f.ident.as_ref().expect("named fields have idents");
                let expr = gen_expr(&f.ty);
                quote! { #name: #expr }
            });
            quote! { #path { #(#pieces),* } }
        }
        Fields::Unnamed(unnamed) => {
            let pieces = unnamed.unnamed.iter().map(|f| gen_expr(&f.ty));
            quote! { #path ( #(#pieces),* ) }
        }
        Fields::Unit => quote! { #path },
    }
}

/// Add `: Bound` to every type parameter of `generics`, leaving lifetimes
/// and const generics alone.
fn add_trait_bound(generics: &Generics, bound: syn::TypeParamBound) -> Generics {
    let mut generics = generics.clone();
    for param in &mut generics.params {
        if let GenericParam::Type(t) = param {
            t.bounds.push(bound.clone());
        }
    }
    generics
}

/// Like `add_trait_bound`, plus ensures the named lifetime outlives every
/// other generic lifetime — required for the `DeserializeView<'a>` bound on
/// generic type parameters.
fn add_trait_bound_with_lifetime(
    generics: &Generics,
    bound: syn::TypeParamBound,
    _lifetime: &Lifetime,
) -> Generics {
    let mut generics = generics.clone();
    for param in &mut generics.params {
        if let GenericParam::Type(t) = param {
            t.bounds.push(bound.clone());
        }
    }
    generics
}

/// Return the single lifetime parameter of `generics`, or `None` if there
/// are zero or more than one.
fn extract_single_lifetime(generics: &Generics) -> Option<Lifetime> {
    let lifetimes: Vec<&LifetimeParam> = generics
        .params
        .iter()
        .filter_map(|p| {
            if let GenericParam::Lifetime(l) = p {
                Some(l)
            } else {
                None
            }
        })
        .collect();
    if lifetimes.len() == 1 {
        Some(lifetimes[0].lifetime.clone())
    } else {
        None
    }
}
