use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Text,
    Markdown,
    Image,
    Unsupported,
}

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "log", "json", "jsonc", "yaml", "yml", "toml", "ini", "cfg", "conf",
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "h", "cpp", "hpp",
    "cs", "rb", "php", "sh", "bat", "ps1", "css", "scss", "html", "xml", "sql",
    "csv", "env",
];

const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown"];

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"];

/// 拡張子からプレビュー種別を判定する。
pub fn classify(path: &Path) -> PreviewKind {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return PreviewKind::Unsupported,
    };

    if MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
        PreviewKind::Markdown
    } else if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        PreviewKind::Text
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        PreviewKind::Image
    } else {
        PreviewKind::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_known_extensions() {
        assert_eq!(classify(&PathBuf::from("a.rs")), PreviewKind::Text);
        assert_eq!(classify(&PathBuf::from("README.md")), PreviewKind::Markdown);
        assert_eq!(classify(&PathBuf::from("photo.PNG")), PreviewKind::Image);
        assert_eq!(classify(&PathBuf::from("archive.zip")), PreviewKind::Unsupported);
        assert_eq!(classify(&PathBuf::from("noext")), PreviewKind::Unsupported);
    }
}
