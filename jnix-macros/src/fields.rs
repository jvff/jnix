use crate::{JnixAttributes, TypeParameters};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse_str, spanned::Spanned, ExprClosure, Field, Fields, Ident, Index, LitStr, Member, Pat,
    PatType, Token, Type,
};

pub struct ParsedField {
    pub name: String,
    pub field: Field,
    pub attributes: JnixAttributes,
    pub member: Member,
    pub source_binding: Ident,
    pub span: Span,
}

impl ParsedField {
    pub fn new(
        name: String,
        field: Field,
        attributes: JnixAttributes,
        member: Member,
        span: Span,
    ) -> Self {
        let source_binding = Ident::new(&format!("_source_{}", name), span);

        ParsedField {
            name,
            field,
            attributes,
            member,
            source_binding,
            span,
        }
    }

    pub fn from_named_field(field: Field) -> Option<Self> {
        let attributes = Self::check_skip_attribute(&field)?;
        let ident = field.ident.clone().expect("Named field with no name ident");
        let span = ident.span();
        let name = ident.to_string();
        let member = Member::Named(ident);

        Some(ParsedField::new(name, field, attributes, member, span))
    }

    pub fn from_unnamed_field((field, index): (Field, u32)) -> Option<Self> {
        let attributes = Self::check_skip_attribute(&field)?;
        let span = field.ty.span();
        let name = format!("_{}", index);
        let member = Member::Unnamed(Index { index, span });

        Some(ParsedField::new(name, field, attributes, member, span))
    }

    fn check_skip_attribute(field: &Field) -> Option<JnixAttributes> {
        let attributes = JnixAttributes::new(&field.attrs);

        if attributes.has_flag("skip") {
            None
        } else {
            Some(attributes)
        }
    }

    pub fn get_type(&self) -> &Type {
        &self.field.ty
    }

    pub fn binding(&self, prefix: &str) -> Ident {
        Ident::new(&format!("_{}_{}", prefix, self.name), self.span)
    }

    pub fn preconversion(&self) -> TokenStream {
        let source = &self.source_binding;

        match self.attributes.get_value("map") {
            Some(closure_string_literal) => {
                let mut closure = parse_str(&closure_string_literal.value())
                    .expect("Invalid closure syntax in jnix(map = ...) attribute");

                self.prepare_map_closure(&mut closure);

                quote! { (#closure)(#source) }
            }
            None => quote! { #source },
        }
    }

    fn prepare_map_closure(&self, closure: &mut ExprClosure) {
        assert!(
            closure.inputs.len() <= 1,
            "Too many parameters in jnix(map = ...) closure"
        );

        let input = closure
            .inputs
            .pop()
            .expect("Missing parameter in jnix(map = ...) closure")
            .into_value();

        closure.inputs.push_value(self.add_type_to_parameter(input));
    }

    fn add_type_to_parameter(&self, parameter: Pat) -> Pat {
        if let &Pat::Type(_) = &parameter {
            parameter
        } else {
            Pat::Type(PatType {
                attrs: vec![],
                pat: Box::new(parameter),
                colon_token: Token![:](Span::call_site()),
                ty: Box::new(self.field.ty.clone()),
            })
        }
    }
}

pub struct ParsedFields {
    fields: Vec<ParsedField>,
}

impl ParsedFields {
    pub fn new(fields: Fields, attributes: JnixAttributes) -> Self {
        ParsedFields {
            fields: Self::collect_parsed_fields(fields, attributes),
        }
    }

    fn collect_parsed_fields(fields: Fields, attributes: JnixAttributes) -> Vec<ParsedField> {
        if attributes.has_flag("skip_all") {
            return vec![];
        }

        match fields {
            Fields::Unit => vec![],
            Fields::Named(fields) => fields
                .named
                .into_iter()
                .filter_map(ParsedField::from_named_field)
                .collect(),
            Fields::Unnamed(fields) => fields
                .unnamed
                .into_iter()
                .zip(0..)
                .filter_map(ParsedField::from_unnamed_field)
                .collect(),
        }
    }

    pub fn generate_struct_variant_into_java(
        &self,
        jni_class_name_literal: &LitStr,
        type_name_literal: &LitStr,
        class_name: String,
        type_parameters: &TypeParameters,
    ) -> TokenStream {
        let source_bindings = self.source_bindings();
        let original_bindings = self.original_bindings();
        let conversion = self.generate_into_java_conversion(
            jni_class_name_literal,
            type_name_literal,
            class_name,
            type_parameters,
        );

        quote! {
            #( let #source_bindings = #original_bindings; )*
            #conversion
        }
    }

    pub fn generate_struct_into_java(
        &self,
        jni_class_name_literal: &LitStr,
        type_name_literal: &LitStr,
        class_name: String,
        type_parameters: &TypeParameters,
    ) -> TokenStream {
        let source_bindings = self.source_bindings();
        let members = self.members();
        let conversion = self.generate_into_java_conversion(
            jni_class_name_literal,
            type_name_literal,
            class_name,
            type_parameters,
        );

        quote! {
            #( let #source_bindings = self.#members; )*
            #conversion
        }
    }

    fn generate_into_java_conversion(
        &self,
        jni_class_name_literal: &LitStr,
        type_name_literal: &LitStr,
        class_name: String,
        type_parameters: &TypeParameters,
    ) -> TokenStream {
        let signature_bindings = self.bindings("signature").collect();
        let final_bindings = self.bindings("final").collect();
        let declarations = self.declarations(&signature_bindings, &final_bindings, type_parameters);

        quote! {
            #( #declarations )*

            let mut constructor_signature = String::with_capacity(
                1 + #( #signature_bindings.as_bytes().len() + )* 2
            );

            constructor_signature.push_str("(");
            #( constructor_signature.push_str(#signature_bindings); )*
            constructor_signature.push_str(")V");
            log::debug!("Constructor signature: {}", constructor_signature);

            let parameters = [ #( jnix::AsJValue::as_jvalue(&#final_bindings) ),* ];

            let class = env.get_class(#jni_class_name_literal);
            let object = env.new_object(&class, constructor_signature, &parameters)
                .expect(concat!("Failed to convert ",
                    #type_name_literal,
                    " Rust type into ",
                    #class_name,
                    " Java object",
                ));

            env.auto_local(object)
        }
    }

    fn declarations<'a, 'b, 'c, 'd, 'z>(
        &'a self,
        signature_bindings: &'b Vec<Ident>,
        final_bindings: &'c Vec<Ident>,
        type_parameters: &'d TypeParameters,
    ) -> impl Iterator<Item = TokenStream> + 'z
    where
        'a: 'z,
        'b: 'z,
        'c: 'z,
        'd: 'z,
    {
        self.fields
            .iter()
            .zip(signature_bindings.iter().zip(final_bindings.iter()))
            .map(move |(field, (signature_binding, final_binding))| {
                let converted_binding = field.binding("converted");
                let conversion = field.preconversion();

                let signature = if let Some(target) = field.attributes.get_value("target_class") {
                    let signature = format!("L{};", target.value().replace(".", "/"));

                    quote! { #signature }
                } else if type_parameters.is_used_in_type(&field.get_type()) {
                    quote! { "Ljava/lang/Object;" }
                } else {
                    quote! { #converted_binding.jni_signature() }
                };

                quote! {
                    let #converted_binding = #conversion;
                    let #signature_binding = #signature;
                    let #final_binding = #converted_binding.into_java(env);
                }
            })
    }

    fn source_bindings(&self) -> impl Iterator<Item = &Ident> + '_ {
        self.fields.iter().map(|field| &field.source_binding)
    }

    fn original_bindings(&self) -> impl Iterator<Item = Ident> + '_ {
        self.fields
            .iter()
            .map(|field| Ident::new(&field.name, field.span))
    }

    fn bindings(&self, prefix: &'static str) -> impl Iterator<Item = Ident> + '_ {
        self.fields.iter().map(move |field| field.binding(prefix))
    }

    fn members(&self) -> impl Iterator<Item = &Member> + '_ {
        self.fields.iter().map(|field| &field.member)
    }
}
