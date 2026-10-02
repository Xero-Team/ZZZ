fn main() {
    let csv_src_dir = std::path::Path::new("src");
    let tsv_src_dir = std::path::Path::new("tsv/src");
    let psv_src_dir = std::path::Path::new("psv/src");
    let semicolon_src_dir = std::path::Path::new("semicolon/src");

    let mut c_config = cc::Build::new();
    c_config
        .std("c11")
        .include(csv_src_dir)
        .include(tsv_src_dir)
        .include(psv_src_dir)
        .include(semicolon_src_dir)
        .flag_if_supported("-Wno-unused-parameter");

    #[cfg(target_env = "msvc")]
    c_config.flag("-utf-8");

    for parser_path in [
        csv_src_dir.join("parser.c"),
        tsv_src_dir.join("parser.c"),
        psv_src_dir.join("parser.c"),
        semicolon_src_dir.join("parser.c"),
    ] {
        c_config.file(&parser_path);
        println!("cargo:rerun-if-changed={}", parser_path.display());
    }

    c_config.compile("tree-sitter-csv");
}
