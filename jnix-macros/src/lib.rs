extern crate proc_macro;

mod attributes;
mod fields;
mod generics;

use crate::{
    attributes::JnixAttributes,
    fields::ParsedFields,
    generics::{ParsedGenerics, TypeParameters},
};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Ident, LitStr, Variant};

#[proc_macro_derive(IntoJava, attributes(jnix))]
pub fn derive_into_java(input: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(input as DeriveInput);
    let attributes = JnixAttributes::new(&parsed_input.attrs);
    let type_name = parsed_input.ident;
    let type_name_literal = LitStr::new(&type_name.to_string(), Span::call_site());
    let class_name = attributes
        .get_value("class_name")
        .expect("Missing Java class name")
        .value();
    let jni_class_name = class_name.replace(".", "/");
    let jni_class_name_literal = LitStr::new(&jni_class_name, Span::call_site());

    let parsed_generics = ParsedGenerics::new(&parsed_input.generics);
    let impl_generics = parsed_generics.impl_generics();
    let trait_generics = parsed_generics.trait_generics();
    let type_generics = parsed_generics.type_generics();
    let where_clause = parsed_generics.where_clause();

    let type_parameters = parsed_generics.type_parameters();

    let debug = attributes.has_flag("debug");

    let into_java_body = generate_into_java_body(
        &jni_class_name_literal,
        &type_name_literal,
        class_name,
        attributes,
        parsed_input.data,
        type_parameters,
    );

    let tokens = quote! {
        #[allow(non_snake_case)]
        impl #impl_generics jnix::IntoJava #trait_generics for #type_name #type_generics
        #where_clause
        {
            const JNI_SIGNATURE: &'static str = concat!("L", #jni_class_name_literal, ";");

            type JavaType = jnix::jni::objects::AutoLocal<'env, 'borrow>;

            fn into_java(self, env: &'borrow jnix::JnixEnv<'env>) -> Self::JavaType {
                #into_java_body
            }
        }
    };

    if debug {
        panic!("{}", TokenStream::from(tokens));
    } else {
        TokenStream::from(tokens)
    }
}

fn generate_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: &LitStr,
    class_name: String,
    attributes: JnixAttributes,
    data: Data,
    type_parameters: TypeParameters,
) -> TokenStream2 {
    match data {
        Data::Enum(data) => generate_enum_into_java_body(
            jni_class_name_literal,
            type_name_literal,
            class_name,
            data.variants.into_iter().collect(),
            type_parameters,
        ),
        Data::Struct(data) => ParsedFields::new(data.fields, attributes).generate_struct_into_java(
            jni_class_name_literal,
            type_name_literal,
            class_name,
            &type_parameters,
        ),
        Data::Union(_) => panic!("Can't derive IntoJava for unions"),
    }
}

fn generate_enum_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: &LitStr,
    class_name: String,
    variants: Vec<Variant>,
    type_parameters: TypeParameters,
) -> TokenStream2 {
    let (variant_names, variant_parameters, variant_bodies) = generate_enum_variants(
        jni_class_name_literal,
        type_name_literal,
        class_name,
        variants,
        type_parameters,
    );

    quote! {
        match self {
            #(
                Self::#variant_names #variant_parameters => {
                    #variant_bodies
                }
            )*
        }
    }
}

#[derive(Clone)]
enum TargetJavaEnumType {
    Unknown,
    EnumClass(Vec<Ident>),
    SealedClass(Vec<Ident>, Vec<Fields>),
}

fn parse_enum_variants(variants: Vec<Variant>) -> TargetJavaEnumType {
    use TargetJavaEnumType::*;

    variants
        .into_iter()
        .fold(Unknown, |enum_type, variant| match enum_type {
            Unknown => match variant.fields {
                Fields::Unit => EnumClass(vec![variant.ident]),
                fields @ Fields::Named(_) | fields @ Fields::Unnamed(_) => {
                    SealedClass(vec![variant.ident], vec![fields])
                }
            },
            EnumClass(mut variant_names) => {
                variant_names.push(variant.ident);

                match variant.fields {
                    Fields::Unit => EnumClass(variant_names),
                    fields @ Fields::Named(_) | fields @ Fields::Unnamed(_) => {
                        let mut variant_fields = Vec::with_capacity(variant_names.len());

                        variant_fields.resize(variant_names.len() - 1, Fields::Unit);
                        variant_fields.push(fields);

                        SealedClass(variant_names, variant_fields)
                    }
                }
            }
            SealedClass(mut variant_names, mut variant_fields) => {
                variant_names.push(variant.ident);
                variant_fields.push(variant.fields);

                SealedClass(variant_names, variant_fields)
            }
        })
}

fn generate_enum_variants(
    jni_class_name_literal: &LitStr,
    type_name_literal: &LitStr,
    class_name: String,
    variants: Vec<Variant>,
    type_parameters: TypeParameters,
) -> (Vec<Ident>, Vec<Option<TokenStream2>>, Vec<TokenStream2>) {
    match parse_enum_variants(variants) {
        TargetJavaEnumType::Unknown => {
            panic!("Can't derive IntoJava for an enum type with no variants")
        }
        TargetJavaEnumType::EnumClass(names) => {
            let mut parameters = Vec::with_capacity(names.len());
            let bodies = generate_enum_class_bodies(
                jni_class_name_literal,
                type_name_literal,
                class_name,
                &names,
            );

            parameters.resize(names.len(), None);

            (names, parameters, bodies)
        }
        TargetJavaEnumType::SealedClass(names, fields) => {
            let parameters = generate_enum_parameters(&fields);
            let bodies = generate_sealed_class_bodies(
                jni_class_name_literal,
                type_name_literal,
                class_name,
                &names,
                fields,
                type_parameters,
            );

            (names, parameters, bodies)
        }
    }
}

fn generate_enum_parameters(variant_fields: &Vec<Fields>) -> Vec<Option<TokenStream2>> {
    variant_fields
        .iter()
        .map(|fields| match fields {
            Fields::Unit => None,
            Fields::Named(named_fields) => {
                let names = named_fields
                    .named
                    .iter()
                    .map(|field| field.ident.clone().expect("Named field without a name"));

                Some(quote! { { #( #names ),* } })
            }
            Fields::Unnamed(unnamed_fields) => {
                let count = unnamed_fields.unnamed.len();
                let names = (0..count).map(|id| Ident::new(&format!("_{}", id), Span::call_site()));

                Some(quote! { ( #( #names ),* ) })
            }
        })
        .collect()
}

fn generate_enum_class_bodies(
    jni_class_name_literal: &LitStr,
    type_name_literal: &LitStr,
    class_name: String,
    variant_names: &Vec<Ident>,
) -> Vec<TokenStream2> {
    variant_names
        .iter()
        .map(|variant_name_ident| {
            let variant_name = variant_name_ident.to_string();
            let variant_name_literal = LitStr::new(&variant_name, Span::call_site());

            quote! {
                let variant_field_id = env.get_static_field_id(
                    #jni_class_name_literal,
                    #variant_name_literal,
                    concat!("L", #jni_class_name_literal, ";"),
                ).expect(concat!("Failed to convert ",
                    #type_name_literal, "::", #variant_name_literal,
                    " Rust enum variant into ",
                    #class_name,
                    " Java object",
                ));

                let variant = env.get_static_field_unchecked(
                    #jni_class_name_literal,
                    variant_field_id,
                    jnix::jni::signature::JavaType::Object(#jni_class_name_literal.to_owned()),
                ).expect(concat!("Failed to convert ",
                    #type_name_literal, "::", #variant_name_literal,
                    " Rust enum variant into ",
                    #class_name,
                    " Java object",
                ));

                match variant {
                    jnix::jni::objects::JValue::Object(object) => env.auto_local(object),
                    _ => panic!(concat!("Conversion from ",
                        #type_name_literal, "::", #variant_name_literal,
                        " Rust enum variant into ",
                        #class_name,
                        " Java object returned an invalid result.",
                    )),
                }
            }
        })
        .collect()
}

fn generate_sealed_class_bodies(
    jni_class_name_literal: &LitStr,
    type_name_literal: &LitStr,
    class_name: String,
    variant_names: &Vec<Ident>,
    variant_fields: Vec<Fields>,
    type_parameters: TypeParameters,
) -> Vec<TokenStream2> {
    variant_names
        .iter()
        .zip(variant_fields.into_iter())
        .map(|(variant_name_ident, fields)| {
            let jni_class_name = jni_class_name_literal.value();
            let variant_class_name = format!("{}${}", jni_class_name, variant_name_ident);
            let variant_class_name_literal = LitStr::new(&variant_class_name, Span::call_site());

            ParsedFields::new(fields, JnixAttributes::empty()).generate_struct_variant_into_java(
                &variant_class_name_literal,
                &type_name_literal,
                class_name.clone(),
                &type_parameters,
            )
        })
        .collect()
}
