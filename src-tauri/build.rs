fn main() {
    // Everything64.dll をビルド出力ディレクトリにコピー（Windows のみ）
    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let dll_src = std::path::Path::new(&manifest_dir).join("Everything64.dll");
        let out_dir = std::env::var("OUT_DIR").unwrap();
        // target/debug（または release）直下に置く
        let target_dir = std::path::Path::new(&out_dir)
            .ancestors()
            .nth(3)
            .unwrap()
            .to_path_buf();
        let dll_dst = target_dir.join("Everything64.dll");
        if dll_src.exists() && !dll_dst.exists() {
            std::fs::copy(&dll_src, &dll_dst).expect("Everything64.dll のコピーに失敗");
        }
        println!("cargo:rerun-if-changed=Everything64.dll");
    }

    tauri_build::build()
}
