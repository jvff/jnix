extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Fields, Ident, Index, Lit, LitStr, Member,
    MetaNameValue,
};

#[proc_macro_derive(IntoJava, attributes(jnix))]
pub fn derive_into_java(input: TokenStream) -> TokenStream {
    let parsed_input = parse_macro_input!(input as DeriveInput);
    let type_name = parsed_input.ident;
    let type_name_literal = LitStr::new(&type_name.to_string(), Span::call_site());
    let class_name = parse_java_class_name(&parsed_input.attrs).expect("Missing Java class name");
    let jni_class_name = class_name.replace(".", "/");
    let jni_class_name_literal = LitStr::new(&jni_class_name, Span::call_site());

    let fields = extract_struct_fields(parsed_input.data);
    let (parameter_conversion, parameter_signatures, parameters) = generate_parameters(fields);

    let tokens = quote! {
        impl<'borrow, 'env: 'borrow> jnix::IntoJava<'borrow, 'env> for #type_name {
            const JNI_SIGNATURE: &'static str = concat!("L", #jni_class_name_literal, ";");

            type JavaType = jnix::jni::objects::JObject<'env>;

            fn into_java(self, env: &'borrow jnix::jni::JNIEnv<'env>) -> Self::JavaType {
                let mut constructor_signature = String::with_capacity(
                    1 + #( #parameter_signatures.as_bytes().len() + )* 2
                );

                constructor_signature.push_str("(");
                #( constructor_signature.push_str(#parameter_signatures); )*
                constructor_signature.push_str(")V");

                #( #parameter_conversion )*

                let parameters = [ #( jnix::AsJValue::as_jvalue(&#parameters) ),* ];

                env.new_object(#jni_class_name_literal, constructor_signature, &parameters)
                    .expect(concat!(
                        "Failed to convert ",
                        #type_name_literal,
                        " Rust type into ",
                        #class_name,
                        " Java object",
                    ))
            }
        }
    };

    TokenStream::from(tokens)
}

fn extract_jnix_attributes(
    attributes: &Vec<Attribute>,
) -> impl Iterator<Item = MetaNameValue> + '_ {
    let jnix_ident = Ident::new("jnix", Span::call_site());

    attributes
        .iter()
        .filter(move |attribute| attribute.path.is_ident(&jnix_ident))
        .map(|attribute| attribute.parse_args().expect("Invalid jnix attribute"))
}

fn parse_java_class_name(attributes: &Vec<Attribute>) -> Option<String> {
    let class_name_ident = Ident::new("class_name", Span::call_site());
    let attribute = extract_jnix_attributes(attributes)
        .find(|attribute| attribute.path.is_ident(&class_name_ident))?;

    if let Lit::Str(class_name) = attribute.lit {
        Some(class_name.value())
    } else {
        None
    }
}

fn extract_struct_fields(data: Data) -> Fields {
    match data {
        Data::Struct(data) => data.fields,
        _ => panic!("Dervie(IntoJava) only supported on structs"),
    }
}

fn generate_parameters(
    fields: Fields,
) -> (Vec<TokenStream2>, Vec<TokenStream2>, Vec<TokenStream2>) {
    let named_fields = match fields {
        Fields::Unit => vec![],
        Fields::Unnamed(fields) => fields
            .unnamed
            .into_iter()
            .zip(0..)
            .map(|(field, counter)| {
                let index = Index {
                    index: counter,
                    span: Span::call_site(),
                };
                let name = Member::Unnamed(index);
                let binding = Ident::new(&format!("_{}", counter), Span::call_site());

                (name, binding, field)
            })
            .collect(),
        Fields::Named(fields) => fields
            .named
            .into_iter()
            .map(|field| {
                let binding = field.ident.clone().expect("Named field with no name");
                let name = Member::Named(binding.clone());

                (name, binding, field)
            })
            .collect(),
    };

    let mut declarations = Vec::with_capacity(named_fields.len());
    let mut signatures = Vec::with_capacity(named_fields.len());
    let mut parameters = Vec::with_capacity(named_fields.len());

    for (name, binding, field) in named_fields {
        let field_type = field.ty;

        declarations.push(quote! { let #binding = self.#name.into_java(env); });
        signatures.push(quote! { <#field_type as jnix::IntoJava>::JNI_SIGNATURE });
        parameters.push(quote! { #binding });
    }

    (declarations, signatures, parameters)
}
