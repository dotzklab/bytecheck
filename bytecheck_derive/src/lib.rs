//! Procedural macros for bytecheck.

#![deny(
    rust_2018_compatibility,
    rust_2018_idioms,
    future_incompatible,
    nonstandard_style,
    unused,
    clippy::all
)]

mod repr;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    meta::ParseNestedMeta, parenthesized, parse::Parse, parse_macro_input,
    parse_quote, punctuated::Punctuated, spanned::Spanned, AttrStyle, Data,
    DeriveInput, Error, Expr, Field, Fields, Ident, Index, LitStr, Path, Token,
    WherePredicate,
};

use repr::Repr;

use crate::repr::BaseRepr;

#[derive(Default)]
struct Attributes {
    pub repr: Repr,
    pub bounds: Option<Punctuated<WherePredicate, Token![,]>>,
    pub crate_path: Option<Path>,
    pub verify: Option<Expr>,
}

fn try_set_attribute<T: ToTokens>(
    attribute: &mut Option<T>,
    value: T,
    name: &'static str,
) -> Result<(), Error> {
    if attribute.is_none() {
        *attribute = Some(value);
        Ok(())
    } else {
        Err(Error::new_spanned(
            value,
            format!("{name} already specified"),
        ))
    }
}

fn parse_check_bytes_attributes(
    attributes: &mut Attributes,
    meta: ParseNestedMeta<'_>,
) -> Result<(), Error> {
    if meta.path.is_ident("bounds") {
        let bounds;
        parenthesized!(bounds in meta.input);
        let bounds =
            bounds.parse_terminated(WherePredicate::parse, Token![,])?;
        try_set_attribute(&mut attributes.bounds, bounds, "bounds")
    } else if meta.path.is_ident("crate") {
        let crate_path = meta.value()?.parse::<LitStr>()?.parse()?;
        try_set_attribute(&mut attributes.crate_path, crate_path, "crate")
    } else if meta.path.is_ident("verify") {
        let verify = meta.value()?.parse::<Expr>()?;
        try_set_attribute(&mut attributes.verify, verify, "verify")
    } else {
        Err(meta.error("unrecognized check_bytes argument"))
    }
}

fn parse_attributes(input: &DeriveInput) -> Result<Attributes, Error> {
    let mut result = Attributes::default();

    for attr in input.attrs.iter() {
        if !matches!(attr.style, AttrStyle::Outer) {
            continue;
        }

        if attr.path().is_ident("check_bytes") {
            attr.parse_nested_meta(|nested| {
                parse_check_bytes_attributes(&mut result, nested)
            })?;
        } else if attr.path().is_ident("repr") {
            attr.parse_nested_meta(|nested| {
                result.repr.parse_list_meta(nested)
            })?;
        }
    }

    Ok(result)
}

/// Derives `CheckBytes` for the labeled type.
///
/// Additional arguments can be specified using the `#[check_bytes(...)]`
/// attribute:
///
/// - `bounds(...)`: Adds additional bounds to the `CheckBytes` implementation.
///   This can be especially useful when dealing with recursive structures,
///   where bounds may need to be omitted to prevent recursive type definitions.
///   In the context of the added bounds, `__C` is the name of the context
///   generic (e.g. `__C: MyContext`).
///
/// This derive macro automatically adds a type bound `field: CheckBytes<__C>`
/// for each field type. This can cause an overflow while evaluating trait
/// bounds if the structure eventually references its own type, as the
/// implementation of `CheckBytes` for a struct depends on each field type
/// implementing it as well. Adding the attribute `#[omit_bounds]` to a field
/// will suppress this trait bound and allow recursive structures. This may be
/// too coarse for some types, in which case additional type bounds may be
/// required with `bounds(...)`.
#[proc_macro_derive(CheckBytes, attributes(check_bytes, omit_bounds))]
pub fn check_bytes_derive(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    match derive_check_bytes(parse_macro_input!(input as DeriveInput)) {
        Ok(result) => result.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn derive_check_bytes(mut input: DeriveInput) -> Result<TokenStream, Error> {
    let attributes = parse_attributes(&input)?;

    let crate_path = attributes.crate_path.unwrap_or(parse_quote!(::bytecheck));

    let mut input_generics = input.generics.clone();
    let impl_where_clause = input_generics.make_where_clause();
    impl_where_clause.predicates.push(match &input.data {
        Data::Struct(_) | Data::Union(_) => parse_quote! {
            <
                __C as #crate_path::rancor::Fallible
            >::Error: #crate_path::rancor::Trace
        },
        Data::Enum(_) => parse_quote! {
            <
                __C as #crate_path::rancor::Fallible
            >::Error: #crate_path::rancor::Error
        },
    });
    if let Some(ref bounds) = attributes.bounds {
        for clause in bounds {
            impl_where_clause.predicates.push(clause.clone());
        }
    }
    input_generics.params.push(parse_quote! {
        __C: #crate_path::rancor::Fallible + ?Sized
    });

    let name = &input.ident;

    let (impl_generics, _, impl_where_clause) = input_generics.split_for_impl();
    let impl_where_clause = impl_where_clause.unwrap();

    input.generics.make_where_clause();
    let (struct_generics, ty_generics, where_clause) =
        input.generics.split_for_impl();
    let where_clause = where_clause.unwrap();

    let verify_check = |check_where| {
        attributes.verify.map(|verify| {
            quote! {
                #[inline(always)]
                fn __verify #impl_generics (
                ) -> impl FnOnce(
                    &#name #ty_generics,
                    &mut __C,
                ) -> ::core::result::Result<
                    (),
                    <__C as #crate_path::rancor::Fallible>::Error,
                >
                #check_where
                {
                    #verify
                }
                __verify()(unsafe { &*value }, context)?;
            }
        })
    };

    let check_bytes_impl = match input.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                let mut check_where = impl_where_clause.clone();
                for field in fields.named.iter().filter(|f| {
                    !f.attrs.iter().any(|a| a.path().is_ident("omit_bounds"))
                }) {
                    let ty = &field.ty;
                    check_where.predicates.push(
                        parse_quote! { #ty: #crate_path::CheckBytes<__C> },
                    );
                }

                let field_checks = fields.named.iter().map(|f| {
                    let field = &f.ident;
                    let ty = &f.ty;
                    quote! {
                        <#ty as #crate_path::CheckBytes<__C>>::check_bytes(
                            ::core::ptr::addr_of!((*value).#field),
                            context
                        ).map_err(|e| {
                            <
                                <
                                    __C as #crate_path::rancor::Fallible
                                >::Error as #crate_path::rancor::Trace
                            >::trace(
                                e,
                                #crate_path::StructCheckContext {
                                    struct_name: ::core::stringify!(#name),
                                    field_name: ::core::stringify!(#field),
                                },
                            )
                        })?;
                    }
                });

                let verify_check = verify_check(&check_where);

                quote! {
                    #[automatically_derived]
                    // SAFETY: `check_bytes` only returns `Ok` if all of the
                    // fields of the struct are valid. If all of the fields are
                    // valid, then the overall struct is also valid.
                    unsafe impl #impl_generics
                        #crate_path::CheckBytes<__C> for #name #ty_generics
                    #check_where
                    {
                        unsafe fn check_bytes(
                            value: *const Self,
                            context: &mut __C,
                        ) -> ::core::result::Result<
                            (),
                            <__C as #crate_path::rancor::Fallible>::Error,
                        > {
                            #(#field_checks)*
                            #verify_check
                            Ok(())
                        }
                    }
                }
            }
            Fields::Unnamed(ref fields) => {
                let mut check_where = impl_where_clause.clone();
                for field in fields.unnamed.iter().filter(|f| {
                    !f.attrs.iter().any(|a| a.path().is_ident("omit_bounds"))
                }) {
                    let ty = &field.ty;
                    check_where.predicates.push(parse_quote! {
                        #ty: #crate_path::CheckBytes<__C>
                    });
                }

                let field_checks =
                    fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let ty = &f.ty;
                        let index = Index::from(i);
                        quote! {
                            <
                                #ty as #crate_path::CheckBytes<__C>
                            >::check_bytes(
                                ::core::ptr::addr_of!((*value).#index),
                                context
                            ).map_err(|e| {
                                <
                                    <
                                        __C as #crate_path::rancor::Fallible
                                    >::Error as #crate_path::rancor::Trace
                                >::trace(
                                    e,
                                    #crate_path::TupleStructCheckContext {
                                        tuple_struct_name: ::core::stringify!(
                                            #name
                                        ),
                                        field_index: #i,
                                    },
                                )
                            })?;
                        }
                    });

                let verify_check = verify_check(&check_where);

                quote! {
                    #[automatically_derived]
                    // SAFETY: `check_bytes` only returns `Ok` if all of the
                    // fields of the struct are valid. If all of the fields are
                    // valid, then the overall struct is also valid.
                    unsafe impl #impl_generics
                        #crate_path::CheckBytes<__C> for #name #ty_generics
                    #check_where
                    {
                        unsafe fn check_bytes(
                            value: *const Self,
                            context: &mut __C,
                        ) -> ::core::result::Result<
                            (),
                            <__C as #crate_path::rancor::Fallible>::Error,
                        > {
                            #(#field_checks)*
                            #verify_check
                            Ok(())
                        }
                    }
                }
            }
            Fields::Unit => {
                let verify_check = verify_check(impl_where_clause);

                quote! {
                    #[automatically_derived]
                    // SAFETY: Unit structs are always valid since they have a
                    // size of 0 and no invalid bit patterns.
                    unsafe impl #impl_generics
                        #crate_path::CheckBytes<__C> for #name #ty_generics
                    #impl_where_clause
                    {
                        unsafe fn check_bytes(
                            value: *const Self,
                            context: &mut __C,
                        ) -> ::core::result::Result<
                            (),
                            <__C as #crate_path::rancor::Fallible>::Error,
                        > {
                            #verify_check
                            Ok(())
                        }
                    }
                }
            }
        },
        Data::Enum(ref data) => {
            let repr = match attributes.repr.base_repr {
                None => return Err(Error::new_spanned(
                    name,
                    "enums implementing CheckBytes must have an explicit repr",
                )),
                Some((BaseRepr::Transparent, _)) => {
                    return Err(Error::new_spanned(
                        name,
                        "enums cannot be repr(transparent)",
                    ))
                }
                Some((BaseRepr::C, _)) => {
                    return Err(Error::new_spanned(
                        name,
                        "repr(C) enums are not currently supported",
                    ))
                }
                Some((BaseRepr::Int(i), _)) => i,
            };

            let mut check_where = impl_where_clause.clone();
            for v in data.variants.iter() {
                match v.fields {
                    Fields::Named(ref fields) => {
                        for field in fields.named.iter().filter(|f| {
                            !f.attrs
                                .iter()
                                .any(|a| a.path().is_ident("omit_bounds"))
                        }) {
                            let ty = &field.ty;
                            check_where.predicates.push(parse_quote! {
                                #ty: #crate_path::CheckBytes<__C>
                            });
                        }
                    }
                    Fields::Unnamed(ref fields) => {
                        for field in fields.unnamed.iter().filter(|f| {
                            !f.attrs
                                .iter()
                                .any(|a| a.path().is_ident("omit_bounds"))
                        }) {
                            let ty = &field.ty;
                            check_where.predicates.push(parse_quote! {
                                #ty: #crate_path::CheckBytes<__C>
                            });
                        }
                    }
                    Fields::Unit => (),
                }
            }

            let tag_variant_defs = data.variants.iter().map(|v| {
                let variant = &v.ident;
                if let Some((_, expr)) = &v.discriminant {
                    quote! { #variant = #expr }
                } else {
                    quote! { #variant }
                }
            });

            let discriminant_const_defs = data.variants.iter().map(|v| {
                let variant = &v.ident;
                quote! {
                    #[allow(non_upper_case_globals)]
                    const #variant: #repr = Tag::#variant as #repr;
                }
            });

            let tag_variant_values = data.variants.iter().map(|v| {
                let name = &v.ident;
                quote! { Discriminant::#name }
            });

            let variant_structs = data.variants.iter().map(|v| {
                let variant = &v.ident;
                let variant_name =
                    Ident::new(&format!("Variant{variant}"), v.span());
                match v.fields {
                    Fields::Named(ref fields) => {
                        let fields = fields.named.iter().map(|f| {
                            let name = &f.ident;
                            let ty = &f.ty;
                            quote! { #name: #ty }
                        });
                        quote! {
                            #[repr(C)]
                            struct #variant_name #struct_generics
                            #where_clause
                            {
                                __tag: Tag,
                                #(#fields,)*
                                __phantom: ::core::marker::PhantomData<
                                    #name #ty_generics
                                >,
                            }
                        }
                    }
                    Fields::Unnamed(ref fields) => {
                        let fields = fields.unnamed.iter().map(|f| {
                            let ty = &f.ty;
                            quote! { #ty }
                        });
                        quote! {
                            #[repr(C)]
                            struct #variant_name #struct_generics (
                                Tag,
                                #(#fields,)*
                                ::core::marker::PhantomData<#name #ty_generics>
                            ) #where_clause;
                        }
                    }
                    Fields::Unit => quote! {},
                }
            });

            let check_arms = data.variants.iter().map(|v| {
                let variant = &v.ident;
                let variant_name =
                    Ident::new(&format!("Variant{variant}"), v.span());
                match v.fields {
                    Fields::Named(ref fields) => {
                        let checks = fields.named.iter().map(|f| {
                            check_arm_named_field(f, &crate_path, name, variant)
                        });
                        quote! { {
                            let value =
                                value.cast::<#variant_name #ty_generics>();
                            #(#checks)*
                        } }
                    }
                    Fields::Unnamed(ref fields) => {
                        let checks =
                            fields.unnamed.iter().enumerate().map(|(i, f)| {
                                check_arm_unnamed_field(
                                    i,
                                    f,
                                    &crate_path,
                                    name,
                                    variant,
                                )
                            });
                        quote! { {
                            let value =
                                value.cast::<#variant_name #ty_generics>();
                            #(#checks)*
                        } }
                    }
                    Fields::Unit => quote! { (), },
                }
            });

            let no_matching_tag_arm = quote! {
                return Err(
                    <
                        <
                            __C as #crate_path::rancor::Fallible
                        >::Error as #crate_path::rancor::Error
                    >::new(
                        #crate_path::InvalidEnumDiscriminantError {
                            enum_name: ::core::stringify!(#name),
                            invalid_discriminant: tag,
                        }
                    )
                )
            };

            let verify_check = verify_check(&check_where);

            quote! {
                const _: () = {
                    #[repr(#repr)]
                    enum Tag {
                        #(#tag_variant_defs,)*
                    }

                    struct Discriminant;

                    #[automatically_derived]
                    impl Discriminant {
                        #(#discriminant_const_defs)*
                    }

                    #(#variant_structs)*

                    #[automatically_derived]
                    // SAFETY: `check_bytes` only returns `Ok` if:
                    // - The discriminant is valid for some variant of the enum,
                    //   and
                    // - Each field of the variant struct is valid.
                    // If the discriminant is valid and the fields of the
                    // indicated variant struct are valid, then the overall enum
                    // is valid.
                    unsafe impl #impl_generics
                        #crate_path::CheckBytes<__C> for #name #ty_generics
                    #check_where
                    {
                        unsafe fn check_bytes(
                            value: *const Self,
                            context: &mut __C,
                        ) -> ::core::result::Result<
                            (),
                            <__C as #crate_path::rancor::Fallible>::Error,
                        > {
                            let tag = *value.cast::<#repr>();
                            match tag {
                                #(#tag_variant_values => #check_arms)*
                                _ => #no_matching_tag_arm,
                            }
                            #verify_check
                            Ok(())
                        }
                    }
                };
            }
        }
        Data::Union(_) => {
            return Err(Error::new(
                input.span(),
                "CheckBytes cannot be derived for unions",
            ));
        }
    };

    Ok(check_bytes_impl)
}

fn check_arm_named_field(
    f: &Field,
    crate_path: &Path,
    name: &Ident,
    variant: &Ident,
) -> TokenStream {
    let field_name = &f.ident;
    let ty = &f.ty;
    quote! {
        <#ty as #crate_path::CheckBytes<__C>>::check_bytes(
            ::core::ptr::addr_of!((*value).#field_name),
            context
        ).map_err(|e| {
            <
                <
                    __C as #crate_path::rancor::Fallible
                >::Error as #crate_path::rancor::Trace
            >::trace(
                e,
                #crate_path::NamedEnumVariantCheckContext {
                    enum_name: ::core::stringify!(#name),
                    variant_name: ::core::stringify!(#variant),
                    field_name: ::core::stringify!(#field_name),
                },
            )
        })?;
    }
}

fn check_arm_unnamed_field(
    i: usize,
    f: &Field,
    crate_path: &Path,
    name: &Ident,
    variant: &Ident,
) -> TokenStream {
    let ty = &f.ty;
    let index = Index::from(i + 1);
    quote! {
        <#ty as #crate_path::CheckBytes<__C>>::check_bytes(
            ::core::ptr::addr_of!((*value).#index),
            context
        ).map_err(|e| {
            <
                <
                    __C as #crate_path::rancor::Fallible
                >::Error as #crate_path::rancor::Trace
            >::trace(
                e,
                #crate_path::UnnamedEnumVariantCheckContext {
                    enum_name: ::core::stringify!(#name),
                    variant_name: ::core::stringify!(#variant),
                    field_index: #index,
                },
            )
        })?;
    }
}
