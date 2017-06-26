#[macro_use] extern crate lazy_static;

extern crate glsl_to_spirv;
extern crate proc_macro;
extern crate syn;
extern crate vulkano_shaders;
extern crate regex;

use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::collections::HashMap;
use std::sync::Mutex;

use regex::{Regex, Captures};

use proc_macro::TokenStream;

lazy_static! {
    static ref STRUCT_HASHMAP: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
    static ref RE: Regex = Regex::new(r#"#\[vulkano_struct\((?P<name>[A-Za-z]+)\)\]"#).unwrap();
}

enum SourceKind {
    Src(String),
    Path(String),
}

fn list_repr_c(list: &[syn::NestedMetaItem]) -> bool {
    if let Some(&syn::NestedMetaItem::MetaItem(syn::MetaItem::Word(ref i))) = list.get(0) {
        i == "C" && list.len() == 1
    } else {
        false
    }
}

fn check_repr_c(attrs: &[syn::Attribute]) {
    let mut iter = attrs.iter().filter_map(|attr| match attr.value {
        syn::MetaItem::List(ref i, ref list) if i == "repr" && list_repr_c(list) => Some(()),
        _ => None
    });

    iter.next().expect("Struct must be #[repr(C)]");
}

fn glsl_ty_from_rs_ty(ty: &syn::Ty) -> &'static str {
    // TODO: actually implement this
    "uvec2"
}

#[proc_macro_derive(VulkanoStruct)]
pub fn derive_struct(input: TokenStream) -> TokenStream {
    let syn_item = syn::parse_macro_input(&input.to_string()).unwrap();

    let fields = if let syn::Body::Struct(syn::VariantData::Struct(ref fields)) = syn_item.body {
        fields
    } else {
        panic!("Must derive vulkano struct on a struct");
    };

    check_repr_c(&syn_item.attrs);

    let name = &syn_item.ident;
    let fields_string = fields.iter().map(|field| format!("    {} {};\n", glsl_ty_from_rs_ty(&field.ty), &field.ident.clone().unwrap())).collect::<Vec<_>>().join(";\n");
    let glsl_struct = [format!("{} {{\n", name), fields_string, "}".into()].join("");

    STRUCT_HASHMAP.lock().unwrap().insert(format!("{}", name), glsl_struct);

    "".parse().unwrap()
}

#[proc_macro_derive(VulkanoShader, attributes(src, path, ty))]
pub fn derive_shader(input: TokenStream) -> TokenStream {
    let syn_item = syn::parse_macro_input(&input.to_string()).unwrap();

    let src = {
        let mut iter = syn_item.attrs.iter().filter_map(|attr| match attr.value {
            syn::MetaItem::NameValue(ref i, syn::Lit::Str(ref val, _)) if i == "src" => {
                Some(SourceKind::Src(val.clone()))
            },

            syn::MetaItem::NameValue(ref i, syn::Lit::Str(ref val, _)) if i == "path" => {
                Some(SourceKind::Path(val.clone()))
            },

            _ => None
        });

        let source = iter.next().expect("No source attribute given ; put #[src = \"...\"] or #[path = \"...\"]");

        if iter.next().is_some() {
            panic!("Multiple src or path attributes given ; please provide only one");
        }

        match source {
            SourceKind::Src(src) => src,

            SourceKind::Path(path) => {
                let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or(".".into());
                let full_path = Path::new(&root).join(&path);

                if full_path.is_file() {
                    let mut buf = String::new();
                    File::open(full_path)
                        .and_then(|mut file| file.read_to_string(&mut buf))
                        .expect(&format!("Error reading source from {:?}", path));
                    buf
                } else {
                    panic!("File {:?} was not found ; note that the path must be relative to your Cargo.toml", path);
                }
            }
        }
    };

    let ty_str = syn_item.attrs.iter().filter_map(|attr| match attr.value {
        syn::MetaItem::NameValue(ref i, syn::Lit::Str(ref val, _)) if i == "ty" => {
            Some(val.clone())
        },
        _ => None
    }).next().expect("Can't find `ty` attribute ; put #[ty = \"vertex\"] for example.");

    let ty = match &ty_str[..] {
        "vertex" => glsl_to_spirv::ShaderType::Vertex,
        "fragment" => glsl_to_spirv::ShaderType::Fragment,
        "geometry" => glsl_to_spirv::ShaderType::Geometry,
        "tess_ctrl" => glsl_to_spirv::ShaderType::TessellationControl,
        "tess_eval" => glsl_to_spirv::ShaderType::TessellationEvaluation,
        "compute" => glsl_to_spirv::ShaderType::Compute,
        _ => panic!("Unexpected shader type ; valid values: vertex, fragment, geometry, tess_ctrl, tess_eval, compute")
    };

    println!("{}", src);

    let hashmap = STRUCT_HASHMAP.lock().unwrap();
    let src = RE.replace_all(&src, |captures: &Captures| {
        let name = captures.name("name").unwrap().as_str();
        hashmap.get(name).expect(&format!("No vulkano_struct named {}", name)).clone()
    });

    println!("{}", src);

    let spirv_data = match glsl_to_spirv::compile(&src, ty) {
        Ok(compiled) => compiled,
        Err(message) => panic!("{}\nfailed to compile shader", message),
    };

    vulkano_shaders::reflect("Shader", spirv_data).unwrap().parse().unwrap()
}
