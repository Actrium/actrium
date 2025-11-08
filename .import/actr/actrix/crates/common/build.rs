use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{Attribute, Fields, Type};

fn main() {
    println!("cargo:rerun-if-changed=src/config");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let config_src_path = Path::new(&manifest_dir).join("src/config/mod.rs");
    let template_output_path = Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("../../deploy/tpl/config.template.toml");

    // 确保输出目录存在
    if let Some(parent) = template_output_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).expect("Failed to create template directory");
        }
    }

    match generate_config_template(&config_src_path, &template_output_path) {
        Ok(_) => println!("Configuration template generated successfully"),
        Err(e) => {
            eprintln!("Failed to generate configuration template: {e}");
            std::process::exit(1);
        }
    }
}

fn generate_config_template(
    src_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_content = fs::read_to_string(src_path)?;
    let syntax_tree: syn::File = syn::parse_str(&source_content)?;

    // 构建结构体信息映射
    let mut structs = HashMap::new();
    for item in &syntax_tree.items {
        if let syn::Item::Struct(struct_item) = item {
            structs.insert(struct_item.ident.to_string(), struct_item);
        }
    }

    let mut template_content = String::new();

    // 添加文件头注释
    template_content.push_str("# Actor-RTC 辅助服务配置文件模板\n");
    template_content.push_str("# 此文件由 config crate 的 build.rs 自动生成\n");
    template_content.push_str("# 请根据实际部署环境修改相应的配置值\n\n");

    // 查找 ActrixConfig 结构体并生成模板
    if let Some(auxes_config) = structs.get("ActrixConfig") {
        generate_struct_template(&mut template_content, auxes_config, &structs, "", 0)?;
    }

    fs::write(output_path, template_content)?;
    Ok(())
}

fn generate_struct_template(
    template: &mut String,
    struct_item: &syn::ItemStruct,
    structs: &HashMap<String, &syn::ItemStruct>,
    section_prefix: &str,
    _indent_level: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Fields::Named(fields) = &struct_item.fields {
        for field in &fields.named {
            let field_name = field.ident.as_ref().unwrap().to_string();

            // 添加字段文档注释
            add_doc_comments(template, &field.attrs);

            // 根据字段类型生成配置
            match &field.ty {
                // 处理 Option<T> 类型
                Type::Path(type_path) if is_option_type(type_path) => {
                    let inner_type = extract_option_inner_type(type_path);
                    if structs.contains_key(&inner_type) {
                        // Option<StructType> - 生成注释掉的结构体配置
                        template.push_str(&format!("# [{section_prefix}{field_name}]\n"));
                        if let Some(inner_struct) = structs.get(&inner_type) {
                            generate_optional_struct_fields(
                                template,
                                inner_struct,
                                structs,
                                &format!("{section_prefix}{field_name}."),
                            )?;
                        }
                    } else {
                        // Option<基础类型> - 生成注释掉的字段
                        let default_value =
                            get_default_value_for_field_with_context(&field_name, section_prefix);
                        template.push_str(&format!("# {field_name} = {default_value}\n"));
                    }
                }
                // 处理自定义结构体类型
                Type::Path(type_path) if structs.contains_key(&type_path_to_string(type_path)) => {
                    let struct_type = type_path_to_string(type_path);
                    template.push_str(&format!("\n[{section_prefix}{field_name}]\n"));
                    if let Some(inner_struct) = structs.get(&struct_type) {
                        generate_struct_template(
                            template,
                            inner_struct,
                            structs,
                            &format!("{section_prefix}{field_name}."),
                            0,
                        )?;
                    }
                }
                // 处理基础类型
                _ => {
                    let default_value =
                        get_default_value_for_field_with_context(&field_name, section_prefix);
                    template.push_str(&format!("{field_name} = {default_value}\n"));
                }
            }

            template.push('\n');
        }
    }

    Ok(())
}

fn generate_optional_struct_fields(
    template: &mut String,
    struct_item: &syn::ItemStruct,
    structs: &HashMap<String, &syn::ItemStruct>,
    section_prefix: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Fields::Named(fields) = &struct_item.fields {
        for field in &fields.named {
            let field_name = field.ident.as_ref().unwrap().to_string();

            // 添加字段文档注释（注释掉的）
            add_doc_comments_commented(template, &field.attrs);

            // 生成注释掉的字段配置
            match &field.ty {
                Type::Path(type_path) if structs.contains_key(&type_path_to_string(type_path)) => {
                    // 嵌套结构体 - 生成子section
                    let struct_type = type_path_to_string(type_path);
                    template.push_str(&format!("# [{section_prefix}{field_name}]\n"));
                    if let Some(inner_struct) = structs.get(&struct_type) {
                        generate_optional_struct_fields(
                            template,
                            inner_struct,
                            structs,
                            &format!("{section_prefix}{field_name}."),
                        )?;
                    }
                }
                _ => {
                    let default_value =
                        get_default_value_for_field_with_context(&field_name, section_prefix);
                    template.push_str(&format!("# {field_name} = {default_value}\n"));
                }
            }
        }
    }

    Ok(())
}

fn add_doc_comments(template: &mut String, attrs: &[Attribute]) {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Ok(doc_string) = attr.value() {
                let doc_content = doc_string.to_token_stream().to_string();
                // 移除引号并清理内容
                let cleaned = doc_content.trim_matches('"').trim();
                if !cleaned.is_empty() {
                    template.push_str(&format!("#{cleaned}\n"));
                }
            }
        }
    }
}

fn add_doc_comments_commented(template: &mut String, attrs: &[Attribute]) {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Ok(doc_string) = attr.value() {
                let doc_content = doc_string.to_token_stream().to_string();
                // 移除引号并清理内容
                let cleaned = doc_content.trim_matches('"').trim();
                if !cleaned.is_empty() {
                    template.push_str(&format!("# #{cleaned}\n"));
                }
            }
        }
    }
}

fn is_option_type(type_path: &syn::TypePath) -> bool {
    if let Some(segment) = type_path.path.segments.first() {
        segment.ident == "Option"
    } else {
        false
    }
}

fn extract_option_inner_type(type_path: &syn::TypePath) -> String {
    if let Some(segment) = type_path.path.segments.first() {
        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
            if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                return type_to_string(inner_type);
            }
        }
    }
    "Unknown".to_string()
}

fn type_path_to_string(type_path: &syn::TypePath) -> String {
    type_path.path.segments.last().unwrap().ident.to_string()
}

fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path_to_string(type_path),
        _ => "Unknown".to_string(),
    }
}

fn get_default_value_for_field_with_context(field_name: &str, section_prefix: &str) -> String {
    match field_name {
        // 主配置字段
        "enable" => "15".to_string(),
        "name" => "\"auxes-default\"".to_string(),
        "env" => "\"dev\"".to_string(),
        "user" => "\"auxes\"".to_string(),
        "group" => "\"auxes\"".to_string(),
        "pid" => "\"logs/actrix.pid\"".to_string(),
        "location_tag" => "\"default-location\"".to_string(),
        "sqlite" => "\"database.db\"".to_string(),
        "log_level" => "\"info\"".to_string(),

        // 网络配置字段
        "domain_name" => "\"localhost\"".to_string(),
        "advertised_ip" => "\"127.0.0.1\"".to_string(),
        "ip" => "\"0.0.0.0\"".to_string(),

        // 端口配置 - 根据上下文区分不同的服务
        "port" => {
            if section_prefix.contains("ice") {
                "3478".to_string() // ICE (STUN/TURN) 端口
            } else if section_prefix.contains("https") {
                "8443".to_string() // HTTPS 端口
            } else {
                "8080".to_string() // HTTP 默认端口
            }
        }

        // TURN 配置
        "advertised_port" => "3478".to_string(),
        "cert" => "\"certificates/server.crt\"".to_string(),
        "key" => "\"certificates/server.key\"".to_string(),
        "relay_port_range" => "\"49152-65535\"".to_string(),
        "realm" => "\"actor-rtc.local\"".to_string(),

        // Supervisor 配置
        "associated_id" => "\"\"".to_string(),
        "secret" => "\"\"".to_string(),
        "addr" => "\"ws://localhost:3000/supervisor\"".to_string(),

        // 默认值
        _ => "\"\"".to_string(),
    }
}

#[allow(dead_code)]
fn get_default_value_for_field(field_name: &str) -> String {
    get_default_value_for_field_with_context(field_name, "")
}

trait AttributeValue {
    fn value(&self) -> Result<TokenStream, syn::Error>;
}

impl AttributeValue for Attribute {
    fn value(&self) -> Result<TokenStream, syn::Error> {
        self.parse_args()
    }
}
