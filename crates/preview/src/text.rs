use std::path::Path;

use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// ファイル先頭から最大 `max_bytes` を読み込み、UTF-8 として解釈する。
///
/// 不正なバイト列（バイナリファイル誤判定など）は置換文字に変換して返す。
/// 戻り値の bool はファイル全体を読み切らなかったかどうか（truncated）。
pub async fn read_head(path: &Path, max_bytes: usize) -> std::io::Result<(String, bool)> {
    let mut file = File::open(path).await?;
    let mut buf = vec![0u8; max_bytes];
    let mut total = 0;

    loop {
        let n = file.read(&mut buf[total..]).await?;
        if n == 0 {
            break;
        }
        total += n;
        if total == buf.len() {
            break;
        }
    }
    buf.truncate(total);

    let truncated = total == max_bytes && file_has_more(&mut file).await?;
    let content = String::from_utf8_lossy(&buf).into_owned();
    Ok((content, truncated))
}

async fn file_has_more(file: &mut File) -> std::io::Result<bool> {
    let mut probe = [0u8; 1];
    Ok(file.read(&mut probe).await? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn reads_full_small_file() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello world").unwrap();

        let (content, truncated) = read_head(f.path(), 1024).await.unwrap();
        assert_eq!(content, "hello world");
        assert!(!truncated);
    }

    #[tokio::test]
    async fn truncates_large_file() {
        let mut f = NamedTempFile::new().unwrap();
        let data = "a".repeat(2000);
        write!(f, "{data}").unwrap();

        let (content, truncated) = read_head(f.path(), 1024).await.unwrap();
        assert_eq!(content.len(), 1024);
        assert!(truncated);
    }
}
