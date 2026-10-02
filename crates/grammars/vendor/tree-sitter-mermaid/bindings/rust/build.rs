fn main() {
    let source_directory = std::path::Path::new("src");
    let parser_path = source_directory.join("parser.c");

    let mut configuration = cc::Build::new();
    configuration.std("c11").include(source_directory);

    #[cfg(target_env = "msvc")]
    configuration.flag("-utf-8");

    configuration.file(&parser_path);
    println!("cargo:rerun-if-changed={}", parser_path.display());
    configuration.compile("tree-sitter-mermaid");
}
