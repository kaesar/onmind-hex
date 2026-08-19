fn main() {
    // Only build the gRPC codegen when the `grpc` feature is on.
    // Must be a compile-time cfg (not a runtime env check): otherwise the
    // optional `tonic-build` dep isn't linked and `tonic_build` stays unresolved.
    #[cfg(feature = "grpc")]
    {
        // Server contract (hex4w.xdb.Xdb) — what `hex` serves.
        tonic_build::configure()
            .build_server(true)
            .build_client(false)
            .compile_protos(&["proto/xdb.proto"], &["proto"])
            .expect("tonic-build: compile xdb.proto");

        // Client contract (co.onmind.grpc.proto.AbcService) — what `hex` calls.
        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .compile_protos(&["proto/abcp.proto"], &["proto"])
            .expect("tonic-build: compile abcp.proto");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto/xdb.proto");
    println!("cargo:rerun-if-changed=proto/abcp.proto");
}