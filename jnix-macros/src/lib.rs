extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse::Parse, parse_macro_input, parse_str, Attribute, Data, DeriveInput, ExprClosure, Field,
    Fields, Ident, Index, Lit, LitStr, Member, MetaNameValue, Pat, PatType, Path, Token, Type,
    Variant,
};

#[proc_macro_derive(IntoJava, attributes(jnix))]
pub fn derive_into_java(input: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(input as DeriveInput);
    let type_name = parsed_input.ident;
    let type_name_literal = LitStr::new(&type_name.to_string(), Span::call_site());
    let class_name = parse_java_class_name(&parsed_input.attrs).expect("Missing Java class name");
    let jni_class_name = class_name.replace(".", "/");
    let jni_class_name_literal = LitStr::new(&jni_class_name, Span::call_site());

    let into_java_body = generate_into_java_body(
        &jni_class_name_literal,
        type_name_literal,
        class_name,
        parsed_input.attrs,
        parsed_input.data,
    );

    let tokens = quote! {
        impl<'borrow, 'env: 'borrow> jnix::IntoJava<'borrow, 'env> for #type_name {
            const JNI_SIGNATURE: &'static str = concat!("L", #jni_class_name_literal, ";");

            type JavaType = jnix::jni::objects::AutoLocal<'env, 'borrow>;

            fn into_java(self, env: &'borrow jnix::jni::JNIEnv<'env>) -> Self::JavaType {
                #into_java_body
            }
        }
    };

    TokenStream::from(tokens)
}

fn extract_jnix_attributes<T>(attributes: &Vec<Attribute>) -> impl Iterator<Item = T> + '_
where
    T: Parse,
{
    let jnix_ident = Ident::new("jnix", Span::call_site());

    attributes
        .iter()
        .filter(move |attribute| attribute.path.is_ident(&jnix_ident))
        .filter_map(|attribute| attribute.parse_args().ok())
}

fn parse_java_class_name(attributes: &Vec<Attribute>) -> Option<String> {
    let class_name_ident = Ident::new("class_name", Span::call_site());
    let attribute = extract_jnix_attributes(attributes)
        .find(|attribute: &MetaNameValue| attribute.path.is_ident(&class_name_ident))?;

    if let Lit::Str(class_name) = attribute.lit {
        Some(class_name.value())
    } else {
        None
    }
}

fn generate_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    attributes: Vec<Attribute>,
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
    let (variant_names, variant_bodies) = generate_enum_variants(
        jni_class_name_literal,
        type_name_literal,
        class_name,
        variants,
    );

    quote! {
        match self {
            #(
                Self::#variant_names => {
                    #variant_bodies
                }
            )*
        }
    }
}

fn generate_enum_variants(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    variants: Vec<Variant>,
) -> (Vec<Ident>, Vec<TokenStream2>) {
    let mut names = Vec::with_capacity(variants.len());
    let mut bodies = Vec::with_capacity(variants.len());

    for variant in variants {
        let variant_name = variant.ident.to_string();
        let variant_name_literal = LitStr::new(&variant_name, Span::call_site());

        names.push(variant.ident);

        bodies.push(match variant.fields {
            Fields::Unit => {
                quote! {
                    let variant = env.get_static_field(
                        #jni_class_name_literal,
                        #variant_name_literal,
                        concat!("L", #jni_class_name_literal, ";"),
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
            }
            _ => panic!("Only unit variants supported for enums"),
        });
    }

    (names, bodies)
}

fn generate_struct_into_java_body(
    jni_class_name_literal: &LitStr,
    type_name_literal: LitStr,
    class_name: String,
    attributes: Vec<Attribute>,
    fields: Fields,
) -> TokenStream2 {
    let (parameter_declarations, parameter_signatures, parameters) =
        generate_struct_parameters(&attributes, fields);

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
    attributes: &Vec<Attribute>,
    fields: Fields,
) -> (Vec<TokenStream2>, Vec<TokenStream2>, Vec<TokenStream2>) {
    let named_fields = parse_fields(attributes, fields);

    let mut declarations = Vec::with_capacity(named_fields.len());
    let mut signatures = Vec::with_capacity(named_fields.len());
    let mut parameters = Vec::with_capacity(named_fields.len());

    for (name, binding, field) in named_fields {
        let source_binding = Ident::new(&format!("_source_{}", binding), Span::call_site());
        let signature_binding = Ident::new(&format!("_signature_{}", binding), Span::call_site());
        let converted_binding = Ident::new(&format!("_converted_{}", binding), Span::call_site());
        let final_binding = Ident::new(&format!("_final_{}", binding), Span::call_site());

        let conversion = generate_conversion(source_binding.clone(), &field);

        declarations.push(quote! {
            let #source_binding = self.#name;
            let #converted_binding = #conversion;
            let #signature_binding = #converted_binding.jni_signature();
            let #final_binding = #converted_binding.into_java(env);
        });
        signatures.push(quote! { #signature_binding });
        parameters.push(quote! { #final_binding });
    }

    (declarations, signatures, parameters)
}

fn parse_fields(attributes: &Vec<Attribute>, fields: Fields) -> Vec<(Member, String, Field)> {
    let skip_ident = Ident::new("skip", Span::call_site());
    let skip_all_ident = Ident::new("skip_all", Span::call_site());
    let should_skip_all = extract_jnix_attributes(attributes)
        .any(|attribute: Path| attribute.is_ident(&skip_all_ident));

    if should_skip_all {
        return vec![];
    }

    match fields {
        Fields::Unit => vec![],
        Fields::Unnamed(fields) => fields
            .unnamed
            .into_iter()
            .filter(|field| {
                !extract_jnix_attributes(&field.attrs)
                    .any(|attribute: Path| attribute.is_ident(&skip_ident))
            })
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
            .filter(|field| {
                !extract_jnix_attributes(&field.attrs)
                    .any(|attribute: Path| attribute.is_ident(&skip_ident))
            })
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
    let map_ident = Ident::new("map", Span::call_site());
    let conversion = extract_jnix_attributes(&field.attrs)
        .find(|attribute: &MetaNameValue| attribute.path.is_ident(&map_ident))
        .map(|attribute| {
            if let Lit::Str(closure) = attribute.lit {
                parse_str::<ExprClosure>(&closure.value())
                    .expect("Invalid closure syntax in jnix(map = ...) attribute")
            } else {
                panic!("Invalid jnix(map = ...) attribute");
            }
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
