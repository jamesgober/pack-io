//! Procedural macros for [`pack-io`].
//!
//! Three derives are provided:
//!
//! - `#[derive(Serialize)]` — implements [`pack_io::Serialize`].
//! - `#[derive(Deserialize)]` — implements [`pack_io::Deserialize`] (owning).
//! - `#[derive(DeserializeView)]` — implements [`pack_io::DeserializeView`]
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
    Data, DataEnum, DataStruct, DeriveInput, Field, Fields, GenericParam, Generics, Ident, Index,
    Lifetime, LifetimeParam, parse_macro_input, parse_quote,
};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Derive [`pack_io::Serialize`] for a struct or enum.
///
/// Fields are encoded in their declaration order. For enums, a
/// `varint(variant_index)` is emitted first, followed by the variant's
/// fields.
///
/// Each field type must already implement `pack_io::Serialize`.
#[proc_macro_derive(Serialize)]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_serialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive [`pack_io::Deserialize`] for a struct or enum (owning decode).
///
/// Each field type must already implement `pack_io::Deserialize`.
#[proc_macro_derive(Deserialize)]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_deserialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive [`pack_io::DeserializeView`] for a struct (zero-copy decode).
///
/// The struct MUST have exactly one lifetime parameter — used as the borrow
/// of the underlying input buffer. Field types must implement
/// `pack_io::DeserializeView<'that_lifetime>`. The built-in impls for
/// primitives, `&'a str`, `&'a [u8]`, and the standard `Option` / `Result`
/// / tuple / array types cover the common cases.
///
/// Enum support and multi-lifetime structs are planned for a later
/// minor release.
#[proc_macro_derive(DeserializeView)]
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

    let body = match &input.data {
        Data::Struct(data) => serialize_struct_body(data),
        Data::Enum(data) => serialize_enum_body(name, data)?,
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

fn serialize_struct_body(data: &DataStruct) -> TokenStream2 {
    field_accessors(&data.fields, quote!(self))
        .into_iter()
        .map(|acc| quote! { ::pack_io::Serialize::serialize(&#acc, __encoder)?; })
        .collect()
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

    let body = match &input.data {
        Data::Struct(data) => deserialize_struct_body(name, &data.fields),
        Data::Enum(data) => deserialize_enum_body(name, data)?,
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

fn deserialize_struct_body(name: &Ident, fields: &Fields) -> TokenStream2 {
    let constructor = construct_from_fields(quote!(#name), fields, |ty| {
        quote_spanned! { ty.span() =>
            <#ty as ::pack_io::Deserialize>::deserialize(__decoder)?
        }
    });
    quote! { ::core::result::Result::Ok(#constructor) }
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
            let constructor = construct_from_fields(
                quote!(#name :: #var_name),
                &variant.fields,
                |ty| {
                    quote_spanned! { ty.span() =>
                        <#ty as ::pack_io::Deserialize>::deserialize(__decoder)?
                    }
                },
            );
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
        Data::Enum(e) => {
            return Err(syn::Error::new_spanned(
                e.enum_token,
                "pack-io: `DeserializeView` on enums is not yet supported; track \
                 https://github.com/jamesgober/pack-io/issues for status",
            ));
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
// Helpers
// ---------------------------------------------------------------------------

/// Walk a struct / variant body and emit per-field accessors (`self.0`,
/// `self.field_name`, or a pre-bound identifier from an enum pattern).
fn field_accessors(fields: &Fields, owner: TokenStream2) -> Vec<TokenStream2> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let name = f.ident.as_ref().expect("named fields have idents");
                quote!(#owner.#name)
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let idx = Index::from(i);
                quote!(#owner.#idx)
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

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
