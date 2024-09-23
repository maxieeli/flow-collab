#![allow(clippy::all)]
#![allow(unused_attributes)]
#![allow(unused_assignments)]

use crate::internal::ctxt::ASTResult;
use proc_macro2::{Ident, TokenStream};
use quote::ToTokens;
use std::fmt;
use std::fmt::Display;
use syn::Meta::{List, NameValue};
use syn::NestedMeta::Meta;
use syn::{self, punctuated::Punctuated, Fields, LitStr, Path, Token};

pub struct ASTContainer<'a> {
    /// The struct or enum name (without generics).
    pub ident: syn::Ident,
    pub path: Option<String>,
    /// The contents of the struct or enum.
    pub data: ASTData<'a>,
}

impl<'a> ASTContainer<'a> {
    pub fn from_ast(ast_result: &ASTResult, ast: &'a syn::DeriveInput) -> Option<ASTContainer<'a>> {
        let data = match &ast.data {
            syn::Data::Struct(data) => {
                // https://docs.rs/syn/1.0.48/syn/struct.DataStruct.html
                let (style, fields) = struct_from_ast(ast_result, &data.fields);
                ASTData::Struct(style, fields)
            },
            syn::Data::Union(_) => {
                ast_result.error_spanned_by(ast, "Does not support derive for unions");
                return None;
            },
            syn::Data::Enum(data) => {
                // https://docs.rs/syn/1.0.48/syn/struct.DataEnum.html
                ASTData::Enum(enum_from_ast(ast_result, &ast.ident, &data.variants))
            },
        };
        let ident = ast.ident.clone();
        let path = get_key(ast_result, &ident, &ast.attrs);
        let item = ASTContainer { ident, path, data };
        Some(item)
    }
}

pub enum ASTData<'a> {
    Struct(ASTStyle, Vec<ASTField<'a>>),
    Enum(Vec<ASTEnumVariant<'a>>),
}

impl<'a> ASTData<'a> {
    pub fn all_fields(&'a self) -> Box<dyn Iterator<Item = &'a ASTField<'a>> + 'a> {
        match self {
            ASTData::Enum(variants) => {
                Box::new(variants.iter().flat_map(|variant| variant.fields.iter()))
            },
            ASTData::Struct(_, fields) => Box::new(fields.iter()),
        }
    }
}

/// A variant of an enum.
pub struct ASTEnumVariant<'a> {
    pub ident: syn::Ident,
    pub style: ASTStyle,
    pub fields: Vec<ASTField<'a>>,
    pub original: &'a syn::Variant,
}

pub struct ASTField<'a> {
    pub member: syn::Member,
    pub ty: &'a syn::Type,
    pub yrs_attr: YrsAttribute,
    pub original: &'a syn::Field,
}

impl<'a> ASTField<'a> {
    pub fn new(ast_result: &ASTResult, field: &'a syn::Field, index: usize) -> Result<Self, String> {
        Ok(ASTField {
            member: match &field.ident {
                Some(ident) => syn::Member::Named(ident.clone()),
                None => syn::Member::Unnamed(index.into()),
            },
            ty: &field.ty,
            yrs_attr: YrsAttribute::from_ast(ast_result, field),
            original: field,
        })
    }
}

pub const YRS: Symbol = Symbol("yrs");
pub const PRS_TY: Symbol = Symbol("ty");
pub struct YrsAttribute {
    #[allow(dead_code)]
    ty: Option<LitStr>,
}

impl YrsAttribute {
    pub fn from_ast(ast_result: &ASTResult, field: &syn::Field) -> Self {
        let mut ty = ASTFieldAttr::none(ast_result, PRS_TY);
        for meta_item in field
            .attrs
            .iter()
            .flat_map(|attr| get_yrs_nested_meta(ast_result, attr))
            .flatten()
        {
            match &meta_item {
                // Parse '#[yrs(ty = x)]'
                Meta(NameValue(m)) if m.path == PRS_TY => {
                    if let syn::Lit::Str(lit) = &m.lit {
                        ty.set(&m.path, lit.clone());
                    }
                },
                _ => {
                    ast_result.error_spanned_by(meta_item, "unexpected meta in field attribute");
                },
            }
        }
        YrsAttribute { ty: ty.get() }
    }
}
