fn main() {
    println!("cargo:rerun-if-changed=ios/documents.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        cc::Build::new()
            .file("ios/documents.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("offering_documents");
        println!("cargo:rustc-link-lib=framework=UIKit");
        println!("cargo:rustc-link-lib=framework=UniformTypeIdentifiers");
    }
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("compile offering recorder UI");
}
