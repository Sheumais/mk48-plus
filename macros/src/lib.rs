// SPDX-FileCopyrightText: 2021 Softbear, Inc.
// SPDX-License-Identifier: AGPL-3.0-or-later

#![feature(proc_macro_span)]

use convert_case::Casing;
use litrs::StringLit;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde::Deserialize;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs;
use std::path::Path;

#[proc_macro]
pub fn entity_type(item: TokenStream) -> TokenStream {
    let input = item.into_iter().collect::<Vec<_>>();
    if input.len() != 1 {
        let msg = format!("expected exactly one input token, got {}", input.len());
        return quote! { compile_error!(#msg) }.into();
    }
    let string_lit = match StringLit::try_from(&input[0]) {
        // Error if the token is not a string literal
        Err(e) => return e.to_compile_error(),
        Ok(lit) => lit,
    };

    let mut path = proc_macro::Span::call_site().source_file().path();
    path.pop();
    path.push(Path::new(string_lit.value()));

    let json = fs::read_to_string(path).expect("unable to load entity json");

    #[derive(Deserialize)]
    struct Subset {
        kind: String,
        length: f32,
        width: f32,
        #[serde(default)]
        level: u8,
    }

    let entity_data: HashMap<String, Subset> =
        serde_json::from_str(&json).expect("unable to parse entity json");

    let mut max_radius = 0f32;
    let mut max_boat_level = 0u8;

    for (_, subset) in &entity_data {
        max_radius = max_radius.max(subset.length.hypot(subset.width));
        if subset.kind == "boat" {
            max_boat_level = max_boat_level.max(subset.level);
        }
    }

    let mut entity_type_strings: Vec<String> = entity_data.into_keys().collect();
    entity_type_strings.sort();

    let entity_types: Vec<EntityType> = entity_type_strings
        .iter()
        .map(|s| EntityType::new(s.to_string()))
        .collect();
    let entity_type_tostrings: Vec<EntityTypeToString> = entity_type_strings
        .iter()
        .map(|s| EntityTypeToString::new(s.to_string()))
        .collect();
    let entity_type_tostrs: Vec<EntityTypeToStr> = entity_type_strings
        .iter()
        .map(|s| EntityTypeToStr::new(s.to_string()))
        .collect();
    //let entity_type_todatas: Vec<EntityTypeToData> = entity_type_strings.iter().map(|s| EntityTypeToData::new(s.to_string())).collect();

    let result = quote! {
        #[repr(u8)]
        #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, enum_iterator::IntoEnumIterator, serde::Serialize, serde::Deserialize)]
        pub enum EntityType {
            #(#entity_types),*
        }

        impl EntityType {
            pub fn to_str(&self) -> &'static str {
                match self {
                    #(#entity_type_tostrs),*
                }
            }

            /*
            pub fn data(&self) -> &EntityData {
                match self {
                    #(#entity_tpye_todatas),*
                }
            }
             */
        }

        impl ToString for EntityType {
            fn to_string(&self) -> String {
                match self {
                    #(#entity_type_tostrings),*
                }
            }
        }

        impl EntityData {
            pub const MAX_RADIUS: f32 = #max_radius;
            pub const MAX_BOAT_LEVEL: u8 = #max_boat_level;
        }
    };
    result.into()
}

struct EntityType(String);

impl EntityType {
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl quote::ToTokens for EntityType {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.to_owned();
        let ident = name_to_ident(name.to_owned());

        let ts: proc_macro2::TokenStream = {
            quote! {
               #[serde(rename = #name)] #ident
            }
        }
        .into();

        tokens.extend(ts);
    }
}

struct EntityTypeToString(String);

impl EntityTypeToString {
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl quote::ToTokens for EntityTypeToString {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.to_owned();
        let ident = name_to_ident(name.to_owned());

        let ts: proc_macro2::TokenStream = {
            quote! {
               EntityType::#ident => String::from(#name)
            }
        }
        .into();

        tokens.extend(ts);
    }
}

struct EntityTypeToStr(String);

impl EntityTypeToStr {
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl quote::ToTokens for EntityTypeToStr {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.to_owned();
        let ident = name_to_ident(name.to_owned());

        let ts: proc_macro2::TokenStream = {
            quote! {
               EntityType::#ident => #name
            }
        }
        .into();

        tokens.extend(ts);
    }
}

/*
struct EntityTypeToData(String);

impl EntityTypeToData {
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl quote::ToTokens for EntityTypeToData {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let name = self.0.to_owned();
        let ident = name_to_ident(name.to_owned());

        let ts: proc_macro2::TokenStream = {
            quote! {
               EntityType::#ident => {
                    const data = EntityData{

                    }
                    &data
               }
            }
        }.into();

        tokens.extend(ts);
    }
}
 */

fn name_to_ident(mut name: String) -> proc_macro2::Ident {
    name = name.replace("0", "Zero");
    name = name.replace("1", "One");
    name = name.replace("2", "Two");
    name = name.replace("3", "Three");
    name = name.replace("4", "Four");
    name = name.replace("5", "Five");
    name = name.replace("6", "Six");
    name = name.replace("7", "Seven");
    name = name.replace("8", "Eight");
    name = name.replace("9", "Nine");
    let upper_camel = name.to_case(convert_case::Case::UpperCamel);
    proc_macro2::Ident::new(&upper_camel, Span::call_site())
}
