fn main() {
    tonic_build::compile_protos("rpc/home.proto").unwrap()
}
