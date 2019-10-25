extern crate proc_macro;

mod attributes;
mod generics;

use crate::{attributes::JnixAttributes, generics::ParsedGenerics};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse_macro_input, parse_str, Data, DeriveInput, ExprClosure, Field, Fields, Ident, Index,
    LitStr, Member, Pat, PatType, Token, Type, Variant,
};

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

    let into_java_body = generate_into_java_body(
        &jni_class_name_literal,
        type_name_literal,
        class_name,
        attributes,
        parsed_input.data,
    );

    let parsed_generics = ParsedGenerics::new(&parsed_input.generics);
    let impl_generics = parsed_generics.impl_generics();
    let trait_generics = parsed_generics.trait_generics();
    let type_generics = parsed_generics.type_generics();
    let where_clause = parsed_generics.where_clause();

    let tokens = quote! {
        impl #impl_generics jnix::IntoJava #trait_generics for #type_name #type_generics
        #where_clause
        {
            const JNI_SIGNATURE: &'static str = concat!("L", #jni_class_name_literal, ";");

            type JavaType = jnix::jni::objects::AutoLocal<'env, 'borrow>;

            fn into_java(self, env: &'borrow jnix::jni::JNIEnv<'env>) -> Self::JavaType {
                #into_java_body
            }
        }
    };

    TokenStream::from(tokens)
}

fn generate_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    attributes: JnixAttributes,
    data: Data,
) -> TokenStream2 {
    match data {
        Data::Enum(data) => generate_enum_into_java_body(
            jni_class_name_literal,
            type_name_literal,
            class_name,
            data.variants.into_iter().collect(),
        ),
        Data::Struct(data) => generate_struct_into_java_body(
            jni_class_name_literal,
            type_name_literal,
            class_name,
            attributes,
            data.fields,
        ),
        Data::Union(_) => panic!("Can't derive IntoJava for unions"),
    }
}

fn generate_enum_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    variants: Vec<Variant>,
) -> TokenStream2 {
    let (variant_names, variant_parameters, variant_bodies) = generate_enum_variants(
        jni_class_name_literal,
        type_name_literal,
        class_name,
        variants,
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
    type_name_literal: LitStr,
    class_name: String,
    variants: Vec<Variant>,
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
    type_name_literal: LitStr,
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
    type_name_literal: LitStr,
    class_name: String,
    variant_names: &Vec<Ident>,
    variant_fields: Vec<Fields>,
) -> Vec<TokenStream2> {
    variant_names
        .iter()
        .zip(variant_fields.into_iter())
        .map(|(variant_name_ident, fields)| {
            let jni_class_name = jni_class_name_literal.value();
            let variant_class_name = format!("{}${}", jni_class_name, variant_name_ident);
            let variant_class_name_literal = LitStr::new(&variant_class_name, Span::call_site());

            let (
                _,
                original_bindings,
                source_bindings,
                parameter_declarations,
                parameter_signatures,
                parameters,
            ) = generate_struct_parameters(JnixAttributes::empty(), fields);

            let body = generate_struct_or_struct_variant_into_java_body(
                &variant_class_name_literal,
                type_name_literal.clone(),
                class_name.clone(),
                parameter_declarations,
                parameter_signatures,
                parameters,
            );

            quote! {
                #( let #source_bindings = #original_bindings; )*

                #body
            }
        })
        .collect()
}

fn generate_struct_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    attributes: JnixAttributes,
    fields: Fields,
) -> TokenStream2 {
    let (names, _, source_bindings, parameter_declarations, parameter_signatures, parameters) =
        generate_struct_parameters(attributes, fields);

    let body = generate_struct_or_struct_variant_into_java_body(
        jni_class_name_literal,
        type_name_literal,
        class_name,
        parameter_declarations,
        parameter_signatures,
        parameters,
    );

    quote! {
        #( let #source_bindings = self.#names; )*

        #body
    }
}

fn generate_struct_or_struct_variant_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    parameter_declarations: Vec<TokenStream2>,
    parameter_signatures: Vec<TokenStream2>,
    parameters: Vec<TokenStream2>,
) -> TokenStream2 {
    quote! {
        #( #parameter_declarations )*

        let mut constructor_signature = String::with_capacity(
            1 + #( #parameter_signatures.as_bytes().len() + )* 2
        );

        constructor_signature.push_str("(");
        #( constructor_signature.push_str(#parameter_signatures); )*
        constructor_signature.push_str(")V");

        let parameters = [ #( jnix::AsJValue::as_jvalue(&#parameters) ),* ];

        let object = env.new_object(#jni_class_name_literal, constructor_signature, &parameters)
            .expect(concat!("Failed to convert ",
                #type_name_literal,
                " Rust type into ",
                #class_name,
                " Java object",
            ));

        env.auto_local(object)
    }
}

fn generate_struct_parameters(
    attributes: JnixAttributes,
    fields: Fields,
) -> (
    Vec<Member>,
    Vec<Ident>,
    Vec<Ident>,
    Vec<TokenStream2>,
    Vec<TokenStream2>,
    Vec<TokenStream2>,
) {
    let named_fields = parse_fields(attributes, fields);

    let mut names = Vec::with_capacity(named_fields.len());
    let mut original_bindings = Vec::with_capacity(named_fields.len());
    let mut source_bindings = Vec::with_capacity(named_fields.len());
    let mut declarations = Vec::with_capacity(named_fields.len());
    let mut signatures = Vec::with_capacity(named_fields.len());
    let mut parameters = Vec::with_capacity(named_fields.len());

    for (name, binding, field) in named_fields {
        let original_binding = Ident::new(&binding, Span::call_site());
        let source_binding = Ident::new(&format!("_source_{}", binding), Span::call_site());
        let signature_binding = Ident::new(&format!("_signature_{}", binding), Span::call_site());
        let converted_binding = Ident::new(&format!("_converted_{}", binding), Span::call_site());
        let final_binding = Ident::new(&format!("_final_{}", binding), Span::call_site());

        let conversion = generate_conversion(source_binding.clone(), &field);

        names.push(name);
        original_bindings.push(original_binding);
        source_bindings.push(source_binding);
        declarations.push(quote! {
            let #converted_binding = #conversion;
            let #signature_binding = #converted_binding.jni_signature();
            let #final_binding = #converted_binding.into_java(env);
        });
        signatures.push(quote! { #signature_binding });
        parameters.push(quote! { #final_binding });
    }

    (
        names,
        original_bindings,
        source_bindings,
        declarations,
        signatures,
        parameters,
    )
}

fn parse_fields(attributes: JnixAttributes, fields: Fields) -> Vec<(Member, String, Field)> {
    if attributes.has_flag("skip_all") {
        return vec![];
    }

    match fields {
        Fields::Unit => vec![],
        Fields::Unnamed(fields) => fields
            .unnamed
            .into_iter()
            .filter(|field| JnixAttributes::new(&field.attrs).has_flag("skip"))
            .zip(0..)
            .map(|(field, counter)| {
                let index = Index {
                    index: counter,
                    span: Span::call_site(),
                };
                let name = Member::Unnamed(index);
                let binding = format!("_{}", counter);

                (name, binding, field)
            })
            .collect(),
        Fields::Named(fields) => fields
            .named
            .into_iter()
            .filter(|field| JnixAttributes::new(&field.attrs).has_flag("skip"))
            .map(|field| {
                let ident = field.ident.clone().expect("Named field with no name");
                let binding = ident.to_string();
                let name = Member::Named(ident);

                (name, binding, field)
            })
            .collect(),
    }
}

fn generate_conversion(source: Ident, field: &Field) -> TokenStream2 {
    let attributes = JnixAttributes::new(&field.attrs);
    let conversion = attributes.get_value("map").map(|lit_str| {
        parse_str(&lit_str.value()).expect("Invalid closure syntax in jnix(map = ...) attribute")
    });

    if let Some(mut closure) = conversion {
        prepare_map_closure(&mut closure, &field);

        quote! { (#closure)(#source) }
    } else {
        quote! { #source }
    }
}

fn prepare_map_closure(closure: &mut ExprClosure, field: &Field) {
    assert!(
        closure.inputs.len() == 1,
        "Too many parameters in jnix(map = ...) closure"
    );

    let input = closure
        .inputs
        .pop()
        .expect("Missing parameter in jnix(map = ...) closure")
        .into_value();

    closure
        .inputs
        .push_value(add_type_to_parameter(input, &field.ty));
}

fn add_type_to_parameter(parameter: Pat, ty: &Type) -> Pat {
    if let &Pat::Type(_) = &parameter {
        parameter
    } else {
        Pat::Type(PatType {
            attrs: vec![],
            pat: Box::new(parameter),
            colon_token: Token![:](Span::call_site()),
            ty: Box::new(ty.clone()),
        })
    }
}
