use protobuf_codegen::Codegen;

fn main() {
    Codegen::new()
        .protoc()
        .cargo_out_dir("generated")
        .input("src/protos/dev.proto")
        .include("src/protos")
        .run_from_script();
}
