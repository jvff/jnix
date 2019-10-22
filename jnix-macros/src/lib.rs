extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{
    parse_macro_input, parse_str, Attribute, Data, DeriveInput, ExprClosure, Fields, Ident, Index,
    Lit, LitStr, Member, MetaNameValue,
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
    let (parameter_declarations, parameter_signatures, parameters) = generate_parameters(fields);

    let tokens = quote! {
        impl<'borrow, 'env: 'borrow> jnix::IntoJava<'borrow, 'env> for #type_name {
            const JNI_SIGNATURE: &'static str = concat!("L", #jni_class_name_literal, ";");

            type JavaType = jnix::jni::objects::JObject<'env>;

            fn into_java(self, env: &'borrow jnix::jni::JNIEnv<'env>) -> Self::JavaType {
                #( #parameter_declarations )*

                let mut constructor_signature = String::with_capacity(
                    1 + #( #parameter_signatures.as_bytes().len() + )* 2
                );

                constructor_signature.push_str("(");
                #( constructor_signature.push_str(#parameter_signatures); )*
                constructor_signature.push_str(")V");

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
                let binding = format!("_{}", counter);

                (name, binding, field)
            })
            .collect(),
        Fields::Named(fields) => fields
            .named
            .into_iter()
            .map(|field| {
                let ident = field.ident.clone().expect("Named field with no name");
                let binding = ident.to_string();
                let name = Member::Named(ident);

                (name, binding, field)
            })
            .collect(),
    };

    let mut declarations = Vec::with_capacity(named_fields.len());
    let mut signatures = Vec::with_capacity(named_fields.len());
    let mut parameters = Vec::with_capacity(named_fields.len());

    for (name, binding, field) in named_fields {
        let source_binding = Ident::new(&format!("_source_{}", binding), Span::call_site());
        let signature_binding = Ident::new(&format!("_signature_{}", binding), Span::call_site());
        let converted_binding = Ident::new(&format!("_converted_{}", binding), Span::call_site());
        let final_binding = Ident::new(&format!("_final_{}", binding), Span::call_site());

        let conversion = generate_conversion(source_binding.clone(), &field.attrs);

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

fn generate_conversion(source: Ident, attributes: &Vec<Attribute>) -> TokenStream2 {
    let map_ident = Ident::new("map", Span::call_site());
    let conversion = extract_jnix_attributes(attributes)
        .find(|attribute| attribute.path.is_ident(&map_ident))
        .map(|attribute| {
            if let Lit::Str(closure) = attribute.lit {
                parse_str::<ExprClosure>(&closure.value())
                    .expect("Invalid closure syntax in jnix(map = ...) attribute")
            } else {
                panic!("Invalid jnix(map = ...) attribute");
            }
        });

    if let Some(closure) = conversion {
        quote! { (#closure)(#source) }
    } else {
        quote! { #source }
    }
}
