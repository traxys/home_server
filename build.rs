extern crate tower_grpc_build;

fn main() {
    tower_grpc_build::Config::new()
        .enable_server(true)
        .enable_client(true)
        .build(&["rpc/home.proto"], &["rpc"])
        .unwrap_or_else(|e| panic!("Protobuf compilation failed: {}", e))
}
