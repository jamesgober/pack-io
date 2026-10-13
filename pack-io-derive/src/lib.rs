//! Procedural macros for [`pack-io`].
//!
//! Three derives are provided:
//!
//! - `#[derive(Serialize)]` — implements `pack_io::Serialize`.
//! - `#[derive(Deserialize)]` — implements `pack_io::Deserialize` (owning).
//! - `#[derive(DeserializeView)]` — implements `pack_io::DeserializeView`
//!   for zero-copy decoding. Types with one lifetime parameter borrow
//!   through it; types without a lifetime get an impl for every lifetime so
//!   they can be nested inside borrowing views.
//!
//! All three work on structs (named, tuple, unit) and enums (any variant
//! shape). Field order in the source code is the encoded byte order; the
//! wire format adds a `varint(variant_index)` prefix for enums.
//!
//! This crate is not intended to be used directly — depend on
//! [`pack-io`] with the `derive` feature instead, which re-exports the
//! macros at `pack_io::{Serialize, Deserialize, DeserializeView}`.
//!
//! # Attributes
//!
//! All attributes live under `#[pack_io(...)]`. Anything else inside
//! `#[pack_io(...)]` is a compile error, so a typo can never be silently
//! ignored.
//!
//! | Where | Attribute | Effect |
//! |---|---|---|
//! | struct | `version = N` | versioned frame (`varint(N) ++ varint(body_len) ++ body`); `N >= 1` |
//! | struct / enum | `crate = "path"` | path to the `pack_io` crate in generated code (default `::pack_io`) |
//! | field of a versioned struct | `since = N` | field added at version `N` (`N <= version`) |
//! | field of a versioned struct | `retired = N` | from version `N` the field is written as its default and ignored on read; its slot stays on the wire, so every reader stays aligned (`N > since`) |
//! | field of a versioned struct | `removed = N` | the field's bytes leave the wire at version `N` (the 1.x `deprecated` encoding); readers that predate it mis-read the payload, so roll out readers first (`N > since`) |
//! | any field | `skip` | never encoded; decoded as its default |
//! | any field | `default` / `default = "path"` | value used when the field is absent from the payload or retired (needs `skip`, `since`, `retired` or `removed`) |
//! | any field | `with = "module"` | encode / decode through `module::serialize` / `module::deserialize` (and `module::deserialize_view` for views) |
//! | enum variant | `index = N` | explicit variant tag; later variants without one continue from `N + 1` |
//!
//! [`pack-io`]: https://crates.io/crates/pack-io

#![deny(missing_docs)]
#![deny(unused_must_use)]
#![deny(unused_results)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]
#![deny(clippy::dbg_macro)]
#![deny(clippy::unreachable)]
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DataEnum, DeriveInput, Fields, GenericParam, Generics, Ident, Index, Lifetime,
    LifetimeParam, parse_macro_input, parse_quote,
};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Derive `pack_io::Serialize` for a struct or enum.
///
/// Fields are encoded in their declaration order. For enums, a
/// `varint(variant_index)` is emitted first, followed by the variant's
/// fields. The generated impl also overrides `encoded_len` with direct
/// arithmetic.
///
/// Each encoded field type must implement `pack_io::Serialize` (or be
/// handled by a `#[pack_io(with = "...")]` module). See the crate docs for
/// the attribute table.
#[proc_macro_derive(Serialize, attributes(pack_io))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_serialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive `pack_io::Deserialize` for a struct or enum (owning decode).
///
/// Each decoded field type must implement `pack_io::Deserialize`; skipped
/// and version-gated fields must implement `Default` (or name a
/// `#[pack_io(default = "path")]` function). The generated impl counts as
/// one nesting level against `Config::max_depth`.
#[proc_macro_derive(Deserialize, attributes(pack_io))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_deserialize(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derive `pack_io::DeserializeView` for a struct or enum (zero-copy decode).
///
/// A type with exactly one lifetime parameter gets
/// `impl DeserializeView<'that_lifetime>`: its `&'a str` / `&'a [u8]` fields
/// borrow from the input buffer. A type with no lifetime parameter gets an
/// impl for every lifetime, so owned helper types (headers, ids, enums of
/// scalars) can be nested inside borrowing views. More than one lifetime
/// parameter is rejected.
///
/// Field types must implement `pack_io::DeserializeView<'_>`. The wire
/// shape, including `#[pack_io(version = N)]` frames, is identical to
/// `#[derive(Deserialize)]`.
#[proc_macro_derive(DeserializeView, attributes(pack_io))]
pub fn derive_deserialize_view(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_deserialize_view(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

// ---------------------------------------------------------------------------
// Attribute model
// ---------------------------------------------------------------------------

/// `#[pack_io(...)]` on the type itself.
#[derive(Default)]
struct TypeAttrs {
    /// `version = N` — the schema version of a versioned struct.
    version: Option<(u32, Span)>,
    /// `crate = "path"` — override for the `::pack_io` path.
    krate: Option<syn::Path>,
}

/// What a field decodes to when the payload does not carry it.
enum DefaultValue {
    /// `Default::default()` (implicit, or a bare `default`).
    Trait,
    /// `default = "path"` — a zero-argument function.
    Path(syn::ExprPath),
}

/// `#[pack_io(...)]` on a field.
#[derive(Default)]
struct FieldAttrs {
    since: Option<(u32, Span)>,
    /// `removed = N`: bytes leave the wire at `N` (1.x `deprecated`).
    removed: Option<(u32, Span)>,
    /// `retired = N`: slot stays, written as the default from `N`.
    retired: Option<(u32, Span)>,
    skip: Option<Span>,
    default: Option<(DefaultValue, Span)>,
    with: Option<syn::Path>,
}

/// One field with everything the generators need.
struct FieldInfo<'a> {
    /// `Some` for named fields.
    ident: Option<&'a Ident>,
    /// Position in the declaration.
    index: usize,
    ty: &'a syn::Type,
    attrs: FieldAttrs,
}

impl FieldInfo<'_> {
    /// Accessor for this field on `self` (`self.name` / `self.0`).
    fn self_access(&self) -> TokenStream2 {
        match self.ident {
            Some(id) => quote!(self.#id),
            None => {
                let idx = Index::from(self.index);
                quote!(self.#idx)
            }
        }
    }

    /// Hygienic local / binding name for this field. Index-based, so user
    /// field names (including raw identifiers such as `r#type` and names
    /// like `__encoder`) can never collide with generated identifiers.
    fn local(&self) -> Ident {
        format_ident!("__pk_f{}", self.index)
    }

    fn since(&self) -> u32 {
        self.attrs.since.map_or(1, |(v, _)| v)
    }

    /// Is the field's slot present on the wire of a payload at `version`?
    /// A retired field keeps its slot; a removed one does not.
    fn live_at(&self, version: u32) -> bool {
        if self.attrs.skip.is_some() || version < self.since() {
            return false;
        }
        self.attrs.removed.is_none_or(|(r, _)| version < r)
    }

    /// Is the field retired at `version` (slot present, value meaningless)?
    fn retired_at(&self, version: u32) -> bool {
        self.attrs.retired.is_some_and(|(r, _)| version >= r)
    }

    /// Read unconditionally at every version, keeping the value read?
    fn always_live(&self) -> bool {
        self.attrs.skip.is_none()
            && self.since() == 1
            && self.attrs.removed.is_none()
            && self.attrs.retired.is_none()
    }

    /// Expression producing the value used when the field is absent.
    fn default_expr(&self) -> TokenStream2 {
        let ty = self.ty;
        match &self.attrs.default {
            Some((DefaultValue::Path(path), _)) => quote_spanned!(path.span() => #path()),
            _ => quote_spanned!(ty.span() => <#ty as ::core::default::Default>::default()),
        }
    }
}

/// `#[pack_io(...)]` on an enum variant.
#[derive(Default)]
struct VariantAttrs {
    index: Option<(u32, Span)>,
}

/// Parse a `= N` integer argument as a `u32`.
fn parse_u32(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<u32> {
    let lit: syn::LitInt = meta.value()?.parse()?;
    lit.base10_parse()
}

/// Parse a `= "path"` string argument as a path.
fn parse_path_str<T: syn::parse::Parse>(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<T> {
    let lit: syn::LitStr = meta.value()?.parse()?;
    lit.parse()
}

fn duplicate(meta: &syn::meta::ParseNestedMeta<'_>, name: &str) -> syn::Error {
    meta.error(format!("pack-io: duplicate `{name}` attribute"))
}

/// Parse every `#[pack_io(...)]` attribute on the type.
fn parse_type_attrs(attrs: &[Attribute]) -> syn::Result<TypeAttrs> {
    let mut out = TypeAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("pack_io")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("version") {
                if out.version.is_some() {
                    return Err(duplicate(&meta, "version"));
                }
                let span = meta.path.span();
                let v = parse_u32(&meta)?;
                if v == 0 {
                    return Err(meta.error(
                        "pack-io: schema versions start at 1; version 0 is reserved by the wire format",
                    ));
                }
                out.version = Some((v, span));
                Ok(())
            } else if meta.path.is_ident("crate") {
                if out.krate.is_some() {
                    return Err(duplicate(&meta, "crate"));
                }
                out.krate = Some(parse_path_str(&meta)?);
                Ok(())
            } else if meta.path.is_ident("since")
                || meta.path.is_ident("deprecated")
                || meta.path.is_ident("retired")
                || meta.path.is_ident("removed")
                || meta.path.is_ident("skip")
                || meta.path.is_ident("default")
                || meta.path.is_ident("with")
            {
                Err(meta.error(
                    "pack-io: this is a field attribute; only `version` and `crate` are allowed on the type",
                ))
            } else if meta.path.is_ident("index") {
                Err(meta.error("pack-io: `index` belongs on an enum variant, not on the type"))
            } else {
                Err(meta.error(
                    "pack-io: unknown attribute; expected `version = N` or `crate = \"path\"`",
                ))
            }
        })?;
    }
    Ok(out)
}

/// Parse every `#[pack_io(...)]` attribute on a field.
fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("pack_io")) {
        attr.parse_nested_meta(|meta| {
            let span = meta.path.span();
            if meta.path.is_ident("since") {
                if out.since.is_some() {
                    return Err(duplicate(&meta, "since"));
                }
                let v = parse_u32(&meta)?;
                if v == 0 {
                    return Err(meta.error("pack-io: schema versions start at 1, not 0"));
                }
                out.since = Some((v, span));
                Ok(())
            } else if meta.path.is_ident("retired") || meta.path.is_ident("removed") {
                let retired = meta.path.is_ident("retired");
                let slot = if retired { &mut out.retired } else { &mut out.removed };
                if slot.is_some() {
                    return Err(duplicate(&meta, if retired { "retired" } else { "removed" }));
                }
                let v = parse_u32(&meta)?;
                if v == 0 {
                    return Err(meta.error("pack-io: schema versions start at 1, not 0"));
                }
                *slot = Some((v, span));
                Ok(())
            } else if meta.path.is_ident("deprecated") {
                Err(meta.error(
                    "pack-io 2.0 replaced `deprecated = N` with two explicit choices: \
                     `retired = N` keeps the field's slot on the wire and writes its default from \
                     version N, which every reader decodes correctly; `removed = N` drops the \
                     field's bytes exactly as 1.x `deprecated` did (same wire format), which readers \
                     built before the change mis-read, so roll readers out first. To keep the bytes \
                     your 1.x code produced, write `removed = N`",
                ))
            } else if meta.path.is_ident("skip") {
                if out.skip.is_some() {
                    return Err(duplicate(&meta, "skip"));
                }
                out.skip = Some(span);
                Ok(())
            } else if meta.path.is_ident("default") {
                if out.default.is_some() {
                    return Err(duplicate(&meta, "default"));
                }
                let value = if meta.input.peek(syn::Token![=]) {
                    DefaultValue::Path(parse_path_str(&meta)?)
                } else {
                    DefaultValue::Trait
                };
                out.default = Some((value, span));
                Ok(())
            } else if meta.path.is_ident("with") {
                if out.with.is_some() {
                    return Err(duplicate(&meta, "with"));
                }
                out.with = Some(parse_path_str(&meta)?);
                Ok(())
            } else if meta.path.is_ident("version") || meta.path.is_ident("crate") {
                Err(meta.error(
                    "pack-io: this is a type-level attribute; put it on the struct or enum",
                ))
            } else if meta.path.is_ident("index") {
                Err(meta.error("pack-io: `index` belongs on an enum variant, not on a field"))
            } else {
                Err(meta.error(
                    "pack-io: unknown field attribute; expected `since`, `retired`, `removed`, `skip`, `default`, or `with`",
                ))
            }
        })?;
    }

    if let Some(skip) = out.skip {
        if out.with.is_some() {
            return Err(syn::Error::new(
                skip,
                "pack-io: `skip` and `with` cannot be combined (a skipped field is never encoded)",
            ));
        }
        if out.since.is_some() || out.removed.is_some() || out.retired.is_some() {
            return Err(syn::Error::new(
                skip,
                "pack-io: `skip` already removes the field from every version; drop `since` / `retired` / `removed`",
            ));
        }
    }
    if let Some((_, span)) = &out.default {
        if out.skip.is_none()
            && out.since.is_none()
            && out.removed.is_none()
            && out.retired.is_none()
        {
            return Err(syn::Error::new(
                *span,
                "pack-io: `default` has no effect on a field that is always encoded; \
                 combine it with `skip`, `since`, `retired`, or `removed`",
            ));
        }
    }
    if let (Some(_), Some((_, span))) = (out.retired, out.removed) {
        return Err(syn::Error::new(
            span,
            "pack-io: a field is either `retired` (slot kept) or `removed` (bytes dropped), not both",
        ));
    }
    for (name, end) in [("retired", out.retired), ("removed", out.removed)] {
        if let (Some((since, _)), Some((n, n_span))) = (out.since, end) {
            if n <= since {
                return Err(syn::Error::new(
                    n_span,
                    format!(
                        "pack-io: `{name} = {n}` must be strictly greater than `since = {since}`; \
                         a field cannot leave before it is introduced",
                    ),
                ));
            }
        }
    }
    Ok(out)
}

/// Parse every `#[pack_io(...)]` attribute on an enum variant.
fn parse_variant_attrs(attrs: &[Attribute]) -> syn::Result<VariantAttrs> {
    let mut out = VariantAttrs::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("pack_io")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("index") {
                if out.index.is_some() {
                    return Err(duplicate(&meta, "index"));
                }
                let span = meta.path.span();
                out.index = Some((parse_u32(&meta)?, span));
                Ok(())
            } else {
                Err(meta.error("pack-io: unknown variant attribute; expected `index = N`"))
            }
        })?;
    }
    Ok(out)
}

/// Collect and validate the fields of a struct or variant.
///
/// `version` is the enclosing struct's schema version (`None` for plain
/// structs and for enum variants, where `since` / `retired` / `removed`
/// are rejected).
fn collect_fields(fields: &Fields, version: Option<u32>) -> syn::Result<Vec<FieldInfo<'_>>> {
    let mut out = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let attrs = parse_field_attrs(&field.attrs)?;
        match version {
            None => {
                if let Some((_, span)) = attrs.since.or(attrs.retired).or(attrs.removed) {
                    return Err(syn::Error::new(
                        span,
                        "pack-io: `since` / `retired` / `removed` are only valid on fields of a \
                         struct that carries `#[pack_io(version = N)]`",
                    ));
                }
            }
            Some(v) => {
                if let Some((since, span)) = attrs.since {
                    if since > v {
                        return Err(syn::Error::new(
                            span,
                            format!(
                                "pack-io: `since = {since}` is newer than the struct's \
                                 `version = {v}`; the field would never be encoded. Bump the \
                                 struct version to {since} or lower `since`",
                            ),
                        ));
                    }
                }
            }
        }
        out.push(FieldInfo {
            ident: field.ident.as_ref(),
            index,
            ty: &field.ty,
            attrs,
        });
    }
    Ok(out)
}

/// One enum variant with its resolved wire index.
struct VariantInfo<'a> {
    ident: &'a Ident,
    index: u32,
    fields: &'a Fields,
    infos: Vec<FieldInfo<'a>>,
}

/// Resolve variant indices (explicit `index = N`, otherwise previous + 1,
/// starting at 0) and reject duplicates.
fn collect_variants(data: &DataEnum) -> syn::Result<Vec<VariantInfo<'_>>> {
    let mut out: Vec<VariantInfo<'_>> = Vec::new();
    let mut next: Option<u32> = Some(0);
    for variant in &data.variants {
        let attrs = parse_variant_attrs(&variant.attrs)?;
        let (index, span) = match attrs.index {
            Some((i, span)) => (i, span),
            None => match next {
                Some(i) => (i, variant.ident.span()),
                None => {
                    return Err(syn::Error::new_spanned(
                        &variant.ident,
                        "pack-io: implicit variant index overflows u32; give this variant an explicit `index`",
                    ));
                }
            },
        };
        if let Some(prev) = out.iter().find(|v| v.index == index) {
            return Err(syn::Error::new(
                span,
                format!(
                    "pack-io: variant index {index} is already used by `{}`",
                    prev.ident
                ),
            ));
        }
        next = index.checked_add(1);
        out.push(VariantInfo {
            ident: &variant.ident,
            index,
            fields: &variant.fields,
            infos: collect_fields(&variant.fields, None)?,
        });
    }
    Ok(out)
}

fn crate_path(attrs: &TypeAttrs) -> TokenStream2 {
    match &attrs.krate {
        Some(path) => quote!(#path),
        None => quote!(::pack_io),
    }
}

fn reject_union(data: &Data) -> syn::Result<()> {
    if let Data::Union(u) = data {
        return Err(syn::Error::new(
            u.union_token.span(),
            "pack-io: `union` is not supported (no defined wire format)",
        ));
    }
    Ok(())
}

fn reject_versioned_enum(attrs: &TypeAttrs, data: &Data) -> syn::Result<()> {
    if let (Data::Enum(_), Some((_, span))) = (data, attrs.version) {
        return Err(syn::Error::new(
            span,
            "pack-io: `#[pack_io(version = N)]` is not supported on enums; wrap the enum in a \
             versioned newtype struct instead",
        ));
    }
    Ok(())
}

fn reject_empty_enum(name: &Ident, data: &DataEnum) -> syn::Result<()> {
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            name,
            "pack-io: empty enums have no values to encode or decode",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Serialize
// ---------------------------------------------------------------------------

fn expand_serialize(input: &DeriveInput) -> syn::Result<TokenStream2> {
    reject_union(&input.data)?;
    let attrs = parse_type_attrs(&input.attrs)?;
    reject_versioned_enum(&attrs, &input.data)?;
    let krate = crate_path(&attrs);
    let name = &input.ident;
    let generics = add_trait_bound(&input.generics, parse_quote!(#krate::Serialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let (write_body, len_body) = match &input.data {
        Data::Struct(data) => {
            let version = attrs.version.map(|(v, _)| v);
            let fields = collect_fields(&data.fields, version)?;
            match version {
                None => serialize_plain_struct(&krate, &fields),
                Some(v) => serialize_versioned_struct(&krate, &fields, v),
            }
        }
        Data::Enum(data) => {
            reject_empty_enum(name, data)?;
            serialize_enum(&krate, &collect_variants(data)?)
        }
        Data::Union(_) => return Err(syn::Error::new_spanned(name, "pack-io: unsupported")),
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #krate::Serialize for #name #ty_generics #where_clause {
            #[inline]
            fn serialize<__E: #krate::Encode + ?Sized>(
                &self,
                __encoder: &mut __E,
            ) -> #krate::Result<()> {
                #write_body
                ::core::result::Result::Ok(())
            }

            #[inline]
            fn encoded_len(&self) -> #krate::Result<usize> {
                #len_body
            }
        }
    })
}

/// `serialize` call for one encoded field reachable through `access`.
fn write_field(krate: &TokenStream2, field: &FieldInfo<'_>, access: &TokenStream2) -> TokenStream2 {
    match &field.attrs.with {
        Some(with) => quote! { #with::serialize(&#access, __encoder)?; },
        None => quote! { #krate::Serialize::serialize(&#access, __encoder)?; },
    }
}

/// Exact encoded length of one encoded field reachable through `access`.
fn field_len(krate: &TokenStream2, field: &FieldInfo<'_>, access: &TokenStream2) -> TokenStream2 {
    match &field.attrs.with {
        Some(with) => quote! {
            #krate::__private::count_with(|__counter| #with::serialize(&#access, __counter))?
        },
        None => quote! { #krate::Serialize::encoded_len(&#access)? },
    }
}

/// Emit `let mut __len` holding the summed encoded lengths of `fields`.
fn sum_lengths<'f, I>(
    krate: &TokenStream2,
    fields: I,
    access: impl Fn(&FieldInfo<'_>) -> TokenStream2,
) -> TokenStream2
where
    I: Iterator<Item = &'f FieldInfo<'f>>,
{
    let adds = fields.map(|f| {
        let len = field_len(krate, f, &access(f));
        quote! { __len = __len.saturating_add(#len); }
    });
    quote! {
        #[allow(unused_mut)]
        let mut __len: usize = 0;
        #(#adds)*
    }
}

fn serialize_plain_struct(
    krate: &TokenStream2,
    fields: &[FieldInfo<'_>],
) -> (TokenStream2, TokenStream2) {
    let encoded = || fields.iter().filter(|f| f.attrs.skip.is_none());
    let writes = encoded().map(|f| write_field(krate, f, &f.self_access()));
    let sum = sum_lengths(krate, encoded(), |f: &FieldInfo<'_>| f.self_access());
    (
        quote! { #(#writes)* },
        quote! {
            #sum
            ::core::result::Result::Ok(__len)
        },
    )
}

fn serialize_versioned_struct(
    krate: &TokenStream2,
    fields: &[FieldInfo<'_>],
    version: u32,
) -> (TokenStream2, TokenStream2) {
    // The body holds exactly the fields live at the writer's version. Its
    // length is computed up front from `encoded_len`, so the fields are
    // written straight into the caller's encoder: no temporary buffer and
    // no copy. The bytes are identical to the 1.0 buffered encoding.
    // A retired field keeps its slot but carries its default value.
    let access = move |f: &FieldInfo<'_>| {
        if f.retired_at(version) {
            let default = f.default_expr();
            quote!((#default))
        } else {
            f.self_access()
        }
    };
    let live = || fields.iter().filter(|f| f.live_at(version));
    let writes = live().map(|f| write_field(krate, f, &access(f)));
    let sum = sum_lengths(krate, live(), access);
    let write_body = quote! {
        #sum
        __encoder.write_varint_u64(#version as u64)?;
        __encoder.write_varint_u64(__len as u64)?;
        #(#writes)*
    };
    let len_body = quote! {
        #sum
        ::core::result::Result::Ok(
            #krate::varint_len(#version as u64)
                .saturating_add(#krate::varint_len(__len as u64))
                .saturating_add(__len),
        )
    };
    (write_body, len_body)
}

/// `Self::Variant { a: __pk_f0, b: _ }` / `Self::Variant(__pk_f0, _)` /
/// `Self::Variant`, binding only the encoded fields.
fn variant_pattern(variant: &VariantInfo<'_>) -> TokenStream2 {
    let var = variant.ident;
    let bind = |f: &FieldInfo<'_>| {
        if f.attrs.skip.is_some() {
            quote!(_)
        } else {
            let local = f.local();
            quote!(#local)
        }
    };
    match variant.fields {
        Fields::Named(_) => {
            let parts = variant.infos.iter().map(|f| {
                let name = f.ident;
                let b = bind(f);
                quote!(#name: #b)
            });
            quote!(Self::#var { #(#parts),* })
        }
        Fields::Unnamed(_) => {
            let parts = variant.infos.iter().map(bind);
            quote!(Self::#var(#(#parts),*))
        }
        Fields::Unit => quote!(Self::#var),
    }
}

fn serialize_enum(
    krate: &TokenStream2,
    variants: &[VariantInfo<'_>],
) -> (TokenStream2, TokenStream2) {
    let local_access = |f: &FieldInfo<'_>| {
        let local = f.local();
        quote!(*#local)
    };
    let write_arms = variants.iter().map(|v| {
        let pattern = variant_pattern(v);
        let index = u64::from(v.index);
        let writes = v
            .infos
            .iter()
            .filter(|f| f.attrs.skip.is_none())
            .map(|f| write_field(krate, f, &local_access(f)));
        quote! {
            #pattern => {
                __encoder.write_varint_u64(#index)?;
                #(#writes)*
            }
        }
    });
    let len_arms = variants.iter().map(|v| {
        let pattern = variant_pattern(v);
        let index = u64::from(v.index);
        let sum = sum_lengths(
            krate,
            v.infos.iter().filter(|f| f.attrs.skip.is_none()),
            local_access,
        );
        quote! {
            #pattern => {
                #sum
                ::core::result::Result::Ok(__len.saturating_add(#krate::varint_len(#index)))
            }
        }
    });
    (
        quote! { match self { #(#write_arms)* } },
        quote! { match self { #(#len_arms)* } },
    )
}

// ---------------------------------------------------------------------------
// Deserialize (owning) and DeserializeView (zero-copy) share one generator;
// `Mode` captures the differences.
// ---------------------------------------------------------------------------

/// Label of the block that holds a generated decode body. Errors leave the
/// block with `break` instead of `?`, so the code after it (which restores
/// the nesting depth) runs on every path. A labelled block optimises better
/// than wrapping the body in a closure.
fn body_label() -> Lifetime {
    Lifetime::new("'__pk_body", Span::call_site())
}

/// `expr?`, but leaving the decode body block instead of the function.
fn try_expr(expr: TokenStream2) -> TokenStream2 {
    let label = body_label();
    quote! {
        match #expr {
            ::core::result::Result::Ok(__pk_v) => __pk_v,
            ::core::result::Result::Err(__pk_e) => break #label ::core::result::Result::Err(__pk_e),
        }
    }
}

enum Mode {
    /// `Deserialize::deserialize` over any `__D: Decode + ?Sized`.
    Owned,
    /// `DeserializeView<'lt>::deserialize_view` over `Decoder<'lt>`.
    View(Lifetime),
}

impl Mode {
    /// Expression decoding one present field from `decoder`.
    fn read(
        &self,
        krate: &TokenStream2,
        field: &FieldInfo<'_>,
        decoder: &TokenStream2,
    ) -> TokenStream2 {
        let ty = field.ty;
        let call = match (self, &field.attrs.with) {
            (Self::Owned, Some(with)) => quote_spanned!(ty.span() => #with::deserialize(#decoder)),
            (Self::View(_), Some(with)) => {
                quote_spanned!(ty.span() => #with::deserialize_view(#decoder))
            }
            (Self::Owned, None) => quote_spanned! { ty.span() =>
                <#ty as #krate::Deserialize>::deserialize(#decoder)
            },
            (Self::View(lt), None) => quote_spanned! { ty.span() =>
                <#ty as #krate::DeserializeView<#lt>>::deserialize_view(#decoder)
            },
        };
        try_expr(call)
    }

    /// Statements that read a versioned body of `__body_len` bytes and bind
    /// `__body_dec`, a decoder over it that keeps the caller's config and
    /// nesting depth.
    fn open_body(&self, krate: &TokenStream2) -> TokenStream2 {
        match self {
            Self::Owned => {
                let body = try_expr(quote!(#krate::Decode::read_byte_cow(__decoder, __body_len)));
                quote! {
                    let __config = #krate::Decode::config(__decoder);
                    let __depth = #krate::Decode::depth(__decoder);
                    let __body = #body;
                    let mut __body_dec = #krate::__private::body_decoder(&__body, __config, __depth);
                }
            }
            Self::View(_) => {
                let body = try_expr(quote!(#krate::Decoder::read_borrowed(__decoder, __body_len)));
                quote! {
                    let __config = #krate::Decode::config(__decoder);
                    let __depth = #krate::Decode::depth(__decoder);
                    let __body = #body;
                    let mut __body_dec = #krate::__private::body_decoder(__body, __config, __depth);
                }
            }
        }
    }

    /// Expression decoding a field of a versioned body from `__body_dec`.
    /// The body decoder is always an in-memory `Decoder`; in owned mode the
    /// field still goes through `Deserialize`.
    fn read_body_field(&self, krate: &TokenStream2, field: &FieldInfo<'_>) -> TokenStream2 {
        self.read(krate, field, &quote!(&mut __body_dec))
    }
}

/// Build `Path { a: expr, .. }` / `Path(expr, ..)` / `Path`.
fn construct(
    path: TokenStream2,
    fields: &Fields,
    infos: &[FieldInfo<'_>],
    expr: impl Fn(&FieldInfo<'_>) -> TokenStream2,
) -> TokenStream2 {
    match fields {
        Fields::Named(_) => {
            let parts = infos.iter().map(|f| {
                let name = f.ident;
                let value = expr(f);
                quote!(#name: #value)
            });
            quote!(#path { #(#parts),* })
        }
        Fields::Unnamed(_) => {
            let parts = infos.iter().map(&expr);
            quote!(#path(#(#parts),*))
        }
        Fields::Unit => path,
    }
}

/// Body of a plain (non-versioned) struct or enum-variant decode: each
/// field in declaration order, skipped fields defaulted.
fn read_plain(
    krate: &TokenStream2,
    mode: &Mode,
    path: TokenStream2,
    fields: &Fields,
    infos: &[FieldInfo<'_>],
) -> TokenStream2 {
    // Bind fields to locals first so the read order is explicit, then
    // build the value.
    let lets = infos.iter().map(|f| {
        let local = f.local();
        let ty = f.ty;
        let value = if f.attrs.skip.is_some() {
            f.default_expr()
        } else {
            mode.read(krate, f, &quote!(__decoder))
        };
        quote! { let #local: #ty = #value; }
    });
    let value = construct(path, fields, infos, |f| {
        let local = f.local();
        quote!(#local)
    });
    quote! {
        #(#lets)*
        ::core::result::Result::Ok(#value)
    }
}

/// Body of a versioned struct decode.
fn read_versioned(
    krate: &TokenStream2,
    mode: &Mode,
    name: &Ident,
    fields: &Fields,
    infos: &[FieldInfo<'_>],
    version: u32,
) -> TokenStream2 {
    let open = mode.open_body(krate);
    let lets = infos.iter().map(|f| {
        let local = f.local();
        let ty = f.ty;
        let value = if f.always_live() {
            mode.read_body_field(krate, f)
        } else if f.attrs.skip.is_some() {
            f.default_expr()
        } else {
            let since = f.since();
            let read = mode.read_body_field(krate, f);
            let default = f.default_expr();
            let present = match f.attrs.removed {
                Some((r, _)) => quote!((#since..#r).contains(&__version)),
                None => quote!(__version >= #since),
            };
            let kept = match f.attrs.retired {
                // The slot is read (so later fields stay aligned) but a
                // retired value is ignored in favour of the default.
                Some((r, _)) => quote! {{
                    let __pk_slot: #ty = #read;
                    if __version >= #r { #default } else { __pk_slot }
                }},
                None => read,
            };
            quote! {
                if #present { #kept } else { #default }
            }
        };
        quote! { let #local: #ty = #value; }
    });
    let value = construct(quote!(#name), fields, infos, |f| {
        let local = f.local();
        quote!(#local)
    });
    let version_read = try_expr(quote!(#krate::__private::read_version(__decoder)));
    let len_read = try_expr(quote!(#krate::__private::read_body_len(__decoder)));
    let finish = try_expr(quote!(#krate::__private::finish_body(&__body_dec, __version, #version)));
    quote! {
        let __version: u32 = #version_read;
        let __body_len: usize = #len_read;
        #open
        #(#lets)*
        #finish;
        ::core::result::Result::Ok(#value)
    }
}

/// Body of an enum decode: the tag is matched as a `u64`, so an unknown tag
/// is reported with its real value.
fn read_enum(
    krate: &TokenStream2,
    mode: &Mode,
    name: &Ident,
    variants: &[VariantInfo<'_>],
) -> TokenStream2 {
    let arms = variants.iter().map(|v| {
        let index = u64::from(v.index);
        let var = v.ident;
        let body = read_plain(krate, mode, quote!(#name::#var), v.fields, &v.infos);
        quote! { #index => { #body } }
    });
    let kind = name.unraw().to_string();
    let tag = try_expr(quote!(#krate::Decode::read_varint_u64(__decoder)));
    quote! {
        let __tag: u64 = #tag;
        match __tag {
            #(#arms)*
            __other => #krate::__private::unknown_variant(#kind, __other),
        }
    }
}

/// Decode body for any type, wrapped so it counts as one nesting level.
fn decode_body(
    krate: &TokenStream2,
    mode: &Mode,
    input: &DeriveInput,
    attrs: &TypeAttrs,
) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let inner = match &input.data {
        Data::Struct(data) => {
            let version = attrs.version.map(|(v, _)| v);
            let infos = collect_fields(&data.fields, version)?;
            match version {
                None => read_plain(krate, mode, quote!(#name), &data.fields, &infos),
                Some(v) => read_versioned(krate, mode, name, &data.fields, &infos, v),
            }
        }
        Data::Enum(data) => {
            reject_empty_enum(name, data)?;
            read_enum(krate, mode, name, &collect_variants(data)?)
        }
        Data::Union(_) => return Err(syn::Error::new_spanned(name, "pack-io: unsupported")),
    };
    let label = body_label();
    Ok(quote! {
        #krate::Decode::enter_nested(__decoder)?;
        #[allow(unused_labels)]
        let __pk_result: #krate::Result<Self> = #label: {
            #inner
        };
        #krate::Decode::leave_nested(__decoder);
        __pk_result
    })
}

fn expand_deserialize(input: &DeriveInput) -> syn::Result<TokenStream2> {
    reject_union(&input.data)?;
    let attrs = parse_type_attrs(&input.attrs)?;
    reject_versioned_enum(&attrs, &input.data)?;
    let krate = crate_path(&attrs);
    let name = &input.ident;
    let generics = add_trait_bound(&input.generics, parse_quote!(#krate::Deserialize));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let body = decode_body(&krate, &Mode::Owned, input, &attrs)?;

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #krate::Deserialize for #name #ty_generics #where_clause {
            #[inline]
            fn deserialize<__D: #krate::Decode + ?Sized>(
                __decoder: &mut __D,
            ) -> #krate::Result<Self> {
                #body
            }
        }
    })
}

fn expand_deserialize_view(input: &DeriveInput) -> syn::Result<TokenStream2> {
    reject_union(&input.data)?;
    let attrs = parse_type_attrs(&input.attrs)?;
    reject_versioned_enum(&attrs, &input.data)?;
    let krate = crate_path(&attrs);
    let name = &input.ident;

    // One lifetime: borrow through it. None: implement for a fresh one.
    let lifetimes: Vec<&LifetimeParam> = input.generics.lifetimes().collect();
    let (lifetime, mut generics) = match lifetimes.as_slice() {
        [] => {
            let lt = Lifetime::new("'__pk_de", Span::call_site());
            let mut g = input.generics.clone();
            g.params
                .insert(0, GenericParam::Lifetime(LifetimeParam::new(lt.clone())));
            (lt, g)
        }
        [only] => (only.lifetime.clone(), input.generics.clone()),
        _ => {
            return Err(syn::Error::new_spanned(
                &input.generics,
                "pack-io: `DeserializeView` supports at most one lifetime parameter (the borrow \
                 of the input buffer)",
            ));
        }
    };
    generics = add_trait_bound(&generics, parse_quote!(#krate::DeserializeView<#lifetime>));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let body = decode_body(&krate, &Mode::View(lifetime.clone()), input, &attrs)?;

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #krate::DeserializeView<#lifetime> for #name #ty_generics #where_clause {
            #[inline]
            fn deserialize_view(
                __decoder: &mut #krate::Decoder<#lifetime>,
            ) -> #krate::Result<Self> {
                #body
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(src: &str) -> DeriveInput {
        syn::parse_str(src).unwrap()
    }

    fn ser_err(src: &str) -> String {
        expand_serialize(&input(src)).unwrap_err().to_string()
    }

    fn de_err(src: &str) -> String {
        expand_deserialize(&input(src)).unwrap_err().to_string()
    }

    fn view_err(src: &str) -> String {
        expand_deserialize_view(&input(src))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn test_plain_struct_expands() {
        let out = expand_serialize(&input("struct S { a: u32, b: String }")).unwrap();
        let text = out.to_string();
        assert!(text.contains("encoded_len"));
        assert!(text.contains(":: pack_io :: Serialize"));
    }

    #[test]
    fn test_crate_attribute_replaces_path() {
        let out = expand_deserialize(&input(
            r#"#[pack_io(crate = "my::reexport")] struct S { a: u32 }"#,
        ))
        .unwrap()
        .to_string();
        assert!(out.contains("my :: reexport :: Deserialize"));
        assert!(!out.contains(":: pack_io ::"));
    }

    #[test]
    fn test_version_zero_is_rejected() {
        assert!(ser_err("#[pack_io(version = 0)] struct S { a: u32 }").contains("version 0"));
    }

    #[test]
    fn test_since_newer_than_version_is_rejected() {
        let err =
            ser_err("#[pack_io(version = 2)] struct S { a: u32, #[pack_io(since = 9)] b: u32 }");
        assert!(err.contains("newer than the struct"), "{err}");
    }

    #[test]
    fn test_since_without_version_is_rejected() {
        let err = de_err("struct S { #[pack_io(since = 2)] a: u32 }");
        assert!(err.contains("only valid on fields of a struct"), "{err}");
    }

    #[test]
    fn test_removed_not_after_since_is_rejected() {
        let err = ser_err(
            "#[pack_io(version = 3)] struct S { #[pack_io(since = 2, removed = 2)] a: u32 }",
        );
        assert!(err.contains("strictly greater"), "{err}");
    }

    #[test]
    fn test_deprecated_is_replaced_by_retired_or_removed() {
        let err = ser_err("#[pack_io(version = 3)] struct S { #[pack_io(deprecated = 3)] a: u32 }");
        assert!(
            err.contains("`retired = N`") && err.contains("`removed = N`"),
            "{err}"
        );
    }

    #[test]
    fn test_retired_and_removed_together_are_rejected() {
        let err = ser_err(
            "#[pack_io(version = 3)] struct S { #[pack_io(retired = 2, removed = 3)] a: u32 }",
        );
        assert!(err.contains("not both"), "{err}");
    }

    #[test]
    fn test_retired_field_expands() {
        let out = expand_serialize(&input(
            "#[pack_io(version = 3)] struct S { a: u32, #[pack_io(retired = 2)] b: String }",
        ))
        .unwrap()
        .to_string();
        assert!(out.contains("Default"), "{out}");
    }

    #[test]
    fn test_unknown_field_attribute_is_rejected() {
        let err = ser_err("struct S { #[pack_io(rename = \"x\")] a: u32 }");
        assert!(err.contains("unknown field attribute"), "{err}");
    }

    #[test]
    fn test_unknown_attribute_on_enum_field_is_rejected() {
        let err = ser_err("enum E { A { #[pack_io(bogus)] x: u32 } }");
        assert!(err.contains("unknown field attribute"), "{err}");
    }

    #[test]
    fn test_since_on_enum_field_is_rejected() {
        let err = de_err("enum E { A { #[pack_io(since = 3)] y: u32 } }");
        assert!(err.contains("only valid on fields of a struct"), "{err}");
    }

    #[test]
    fn test_unknown_variant_attribute_is_rejected() {
        let err = ser_err("enum E { #[pack_io(rename = \"b\")] B }");
        assert!(err.contains("unknown variant attribute"), "{err}");
    }

    #[test]
    fn test_unknown_type_attribute_is_rejected() {
        let err = ser_err("#[pack_io(versoin = 1)] struct S;");
        assert!(err.contains("unknown attribute"), "{err}");
    }

    #[test]
    fn test_duplicate_variant_index_is_rejected() {
        let err = ser_err("enum E { #[pack_io(index = 1)] A, B, #[pack_io(index = 2)] C }");
        assert!(err.contains("already used by `B`"), "{err}");
    }

    #[test]
    fn test_skip_with_is_rejected() {
        let err = ser_err("struct S { #[pack_io(skip, with = \"m\")] a: u32 }");
        assert!(err.contains("cannot be combined"), "{err}");
    }

    #[test]
    fn test_default_without_absence_is_rejected() {
        let err = ser_err("struct S { #[pack_io(default)] a: u32 }");
        assert!(err.contains("has no effect"), "{err}");
    }

    #[test]
    fn test_versioned_enum_is_rejected() {
        let err = ser_err("#[pack_io(version = 1)] enum E { A }");
        assert!(err.contains("not supported on enums"), "{err}");
    }

    #[test]
    fn test_union_is_rejected() {
        assert!(ser_err("union U { a: u32 }").contains("union"));
    }

    #[test]
    fn test_empty_enum_is_rejected() {
        assert!(de_err("enum E {}").contains("empty enums"));
    }

    #[test]
    fn test_view_with_two_lifetimes_is_rejected() {
        assert!(view_err("struct V<'a, 'b> { a: &'a str, b: &'b str }").contains("at most one"));
    }

    #[test]
    fn test_view_without_lifetime_expands() {
        let out = expand_deserialize_view(&input("struct H { id: u64 }"))
            .unwrap()
            .to_string();
        assert!(out.contains("'__pk_de"));
    }

    #[test]
    fn test_raw_identifier_field_expands() {
        let out = expand_deserialize(&input("#[pack_io(version = 1)] struct R { r#type: u32 }"));
        assert!(out.is_ok());
    }

    #[test]
    fn test_duplicate_attribute_is_rejected() {
        let err = ser_err("#[pack_io(version = 1, version = 2)] struct S;");
        assert!(err.contains("duplicate"), "{err}");
    }
}
