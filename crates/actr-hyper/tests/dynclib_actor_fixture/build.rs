use actr_framework_protoc_codegen::{DispatchRoute, DispatchSpec, UnifiedDispatcherGenerator};
use actr_protocol::ReplyMode;
use prost_reflect::{DescriptorPool, Value};
use std::{collections::HashMap, env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "upload.proto";
    let protocol_include = "../../../actr-protocol/proto";
    println!("cargo:rerun-if-changed={proto}");
    println!("cargo:rerun-if-changed={protocol_include}/actr/options.proto");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("upload-descriptors.bin");
    let mut config = prost_build::Config::new();
    config.file_descriptor_set_path(&descriptor_path);
    config.extern_path(".actr", "::actr_protocol");
    let descriptors = config.load_fds(&[proto], &[".", protocol_include])?;

    // The extension is read from the original descriptor bytes because the
    // generated descriptor structs discard unknown protobuf fields.
    let pool = DescriptorPool::decode(fs::read(descriptor_path)?.as_slice())?;
    let reply_option = pool.get_extension_by_name("actr.reply_mode").unwrap();
    let service = pool.get_service_by_name("upload.Upload").unwrap();
    let mut reply_modes = HashMap::new();
    let mut routes = Vec::new();
    for method in service.methods() {
        let Value::EnumNumber(value) = method.options().get_extension(&reply_option).into_owned()
        else {
            panic!("actr.reply_mode must be an enum");
        };
        let mode = ReplyMode::try_from(value)?;
        let route_key = method.full_name().to_owned();
        reply_modes.insert(route_key.clone(), mode);
        routes.push(DispatchRoute {
            route_key,
            proto_file: proto.into(),
            proto_service: service.full_name().into(),
            method: method.name().into(),
            input_type: method.input().full_name().into(),
            output_type: method.output().full_name().into(),
            payload_type: "rpc_reliable".into(),
            reply_mode: match mode {
                ReplyMode::Explicit => "explicit",
                ReplyMode::Return => "return",
            }
            .into(),
            dependency_alias: None,
        });
    }
    let spec = DispatchSpec {
        schema_version: "v1".into(),
        routes,
        spec_hash: None,
    };
    let file = descriptors
        .file
        .iter()
        .find(|file| file.name() == proto)
        .unwrap();
    let methods = file.service[0]
        .method
        .iter()
        .map(|method| (format!("upload.Upload.{}", method.name()), method))
        .collect();
    let dispatcher =
        UnifiedDispatcherGenerator::new(&spec, &HashMap::new(), &reply_modes).generate(&methods)?;
    fs::write(out_dir.join("upload_dispatch.rs"), dispatcher)?;
    config.compile_fds(descriptors)?;
    Ok(())
}
