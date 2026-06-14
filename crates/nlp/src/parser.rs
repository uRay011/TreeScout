/// 自然言語クエリを解析した結果。
/// Everything クエリ構文に変換して使う。
#[derive(Debug, Default, PartialEq)]
pub struct ParsedQuery {
    /// キーワード（ファイル名含む自由テキスト）
    pub keywords: Vec<String>,
    /// 拡張子フィルタ（例: ["pdf", "docx"]）
    pub extensions: Vec<String>,
    /// パスフィルタ（例: "src/"）
    pub path_filter: Option<String>,
    /// 最小サイズ（バイト）
    pub min_size: Option<u64>,
    /// 最大サイズ（バイト）
    pub max_size: Option<u64>,
    /// 相対日付フィルタ（Everything `dm:` 構文に渡す文字列）
    pub date_modified: Option<String>,
}

impl ParsedQuery {
    /// Everything クエリ文字列に変換する。
    /// 例: `ext:pdf dm:lastweek "project report"`
    pub fn to_everything_query(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if !self.extensions.is_empty() {
            parts.push(format!("ext:{}", self.extensions.join(",")));
        }
        if let Some(path) = &self.path_filter {
            parts.push(format!("path:{}", path));
        }
        if let Some(min) = self.min_size {
            parts.push(format!("size:>{}", min));
        }
        if let Some(max) = self.max_size {
            parts.push(format!("size:<{}", max));
        }
        if let Some(date) = &self.date_modified {
            parts.push(format!("dm:{}", date));
        }
        for kw in &self.keywords {
            let term = everything_term(kw);
            if term.contains(' ') {
                parts.push(format!("\"{}\"", term));
            } else {
                parts.push(term);
            }
        }

        parts.join(" ")
    }
}

/// Everythingへの問い合わせ語を生成する。
///
/// キーワード前半（最大 `len/2`、最低3文字）のprefixを使うことで、後半に
/// 誤字・脱字・転置があるファイル名も Phase1 候補に含める（タイポ耐性）。
/// 打ち切り後の文字列は元のキーワードの部分文字列なので、完全一致する
/// ファイルも変わらず候補に残る。スコアリング（`score_path`）は元の
/// キーワードで編集距離評価するため、ここでは Everything への問い合わせ
/// 文字列のみを広げる。
///
/// 制約: 前半（保持したprefix部分）に誤字があるケースは捕捉できない
/// （例: "xeport"→"xep" は "report" の部分文字列にならない）。
fn everything_term(kw: &str) -> String {
    let chars: Vec<char> = kw.chars().collect();
    let len = chars.len();
    let keep = (len / 2).max(3);
    if keep >= len {
        return kw.to_string();
    }
    chars[..keep].iter().collect()
}

/// 自然言語クエリをルールベースで解析する。
///
/// 対応パターン:
/// - 日付: 「今日」「昨日」「今週」「先週」「今月」「先月」「去年」「今年」「先週の月曜」等
/// - サイズ: 「N MB 以上」「N KB 以下」「N GB 超」等
/// - 拡張子: 「PDF」「Excel」「Word」「TypeScript」等のエイリアス
/// - パス: 「src/」「path:foo」等
/// - 残余: キーワードとして収集
pub fn parse_query(input: &str) -> ParsedQuery {
    let mut result = ParsedQuery::default();
    let lower = input.to_lowercase();
    let mut remaining_tokens: Vec<&str> = Vec::new();

    // -- 日付パターン --
    let date_mod = extract_date_modifier(&lower);
    result.date_modified = date_mod;

    // -- サイズパターン --
    let (min_size, max_size) = extract_size(&lower);
    result.min_size = min_size;
    result.max_size = max_size;

    // -- 拡張子パターン --
    result.extensions = extract_extensions(&lower);

    // -- パスフィルタ --
    result.path_filter = extract_path(input);

    // -- 残余キーワード --
    // パス・拡張子・日付・サイズとして消費済みのトークンを除外する。
    let consumed_path = result.path_filter.as_deref().unwrap_or("");
    for token in input.split_whitespace() {
        let t = token.to_lowercase();
        if is_noise_token(&t) {
            continue;
        }
        // `path:foo` 直接構文トークンを除外
        if t.starts_with("path:") || t.starts_with("ext:") || t.starts_with("dm:") || t.starts_with("size:") {
            continue;
        }
        // スラッシュ含みトークン（パスとして消費済み）を除外
        if token.contains('/') && !token.starts_with("http") {
            continue;
        }
        // 拡張子エイリアスとして消費済みの単語を除外
        if !result.extensions.is_empty() && is_extension_alias(&t) {
            continue;
        }
        // 日付キーワードを除外
        if result.date_modified.is_some() && is_date_keyword(&t) {
            continue;
        }
        // consumed_path がある場合、そのパスを含むトークンを除外
        if !consumed_path.is_empty() && token.starts_with(consumed_path) {
            continue;
        }
        remaining_tokens.push(token);
    }
    result.keywords = remaining_tokens
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    result
}

// ---- 内部ヘルパー ----

fn extract_date_modifier(lower: &str) -> Option<String> {
    if lower.contains("今日") || lower.contains("本日") || lower.contains("today") {
        return Some("today".to_string());
    }
    if lower.contains("昨日") || lower.contains("yesterday") {
        return Some("yesterday".to_string());
    }
    if lower.contains("今週") || lower.contains("this week") {
        return Some("thisweek".to_string());
    }
    if lower.contains("先週") || lower.contains("last week") || lower.contains("lastweek") {
        return Some("lastweek".to_string());
    }
    if lower.contains("今月") || lower.contains("this month") {
        return Some("thismonth".to_string());
    }
    if lower.contains("先月") || lower.contains("last month") {
        return Some("lastmonth".to_string());
    }
    if lower.contains("今年") || lower.contains("this year") {
        return Some("thisyear".to_string());
    }
    if lower.contains("去年") || lower.contains("昨年") || lower.contains("last year") {
        return Some("lastyear".to_string());
    }
    // dm:YYYY/MM など直接構文もスルーパス
    if let Some(idx) = lower.find("dm:") {
        let rest: &str = &lower[idx + 3..];
        let value: &str = rest.split_whitespace().next().unwrap_or("");
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn extract_size(lower: &str) -> (Option<u64>, Option<u64>) {
    let mut min: Option<u64> = None;
    let mut max: Option<u64> = None;

    // size:>N, size:<N 直接構文
    if let Some(idx) = lower.find("size:>") {
        let rest = &lower[idx + 6..];
        if let Some(v) = parse_size_value(rest) {
            min = Some(v);
        }
    }
    if let Some(idx) = lower.find("size:<") {
        let rest = &lower[idx + 6..];
        if let Some(v) = parse_size_value(rest) {
            max = Some(v);
        }
    }

    // 自然言語: "N MB 以上" / "N MB より大きい" / "N MB over"
    let units: &[(&str, u64)] = &[
        ("gb", 1 << 30),
        ("mb", 1 << 20),
        ("kb", 1024),
    ];
    let above_markers = ["以上", "より大きい", "超", "over", "above", ">"];
    let below_markers = ["以下", "より小さい", "未満", "under", "below", "<"];

    for (unit_str, multiplier) in units {
        if let Some(idx) = lower.find(unit_str) {
            // 数値を左側から探す
            let before = &lower[..idx];
            if let Some(num) = extract_last_number(before) {
                let bytes = num * multiplier;
                let after = &lower[idx + unit_str.len()..];
                let combined = format!("{}{}", unit_str, after);
                if above_markers.iter().any(|m| combined.contains(m) || before.contains(m)) {
                    min = min.or(Some(bytes));
                } else if below_markers.iter().any(|m| combined.contains(m) || before.contains(m)) {
                    max = max.or(Some(bytes));
                }
            }
        }
    }

    (min, max)
}

fn parse_size_value(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let num: u64 = digits.parse().ok()?;
    let rest = s[digits.len()..].trim().to_lowercase();
    let multiplier = if rest.starts_with("gb") {
        1u64 << 30
    } else if rest.starts_with("mb") {
        1u64 << 20
    } else if rest.starts_with("kb") {
        1024
    } else {
        1
    };
    Some(num * multiplier)
}

fn extract_last_number(s: &str) -> Option<u64> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    for token in tokens.iter().rev() {
        if let Ok(n) = token.parse::<u64>() {
            return Some(n);
        }
    }
    None
}

fn extract_extensions(lower: &str) -> Vec<String> {
    let mut exts: Vec<String> = Vec::new();

    // 直接構文: ext:pdf,docx
    if let Some(idx) = lower.find("ext:") {
        let rest = &lower[idx + 4..];
        let value = rest.split_whitespace().next().unwrap_or("");
        for e in value.split(',') {
            let e = e.trim();
            if !e.is_empty() {
                exts.push(e.to_string());
            }
        }
        if !exts.is_empty() {
            return exts;
        }
    }

    // 自然言語エイリアス
    let aliases: &[(&[&str], &[&str])] = &[
        (&["pdf"], &["pdf"]),
        (&["excel", "エクセル", "xlsx", "xls"], &["xlsx", "xls"]),
        (&["word", "ワード", "docx", "doc"], &["docx", "doc"]),
        (&["powerpoint", "パワーポイント", "pptx", "ppt"], &["pptx", "ppt"]),
        (&["python", "パイソン"], &["py"]),
        (&["rust", "ラスト"], &["rs"]),
        (
            &["typescript", "タイプスクリプト", "tsx"],
            &["ts", "tsx"],
        ),
        (&["javascript", "ジャバスクリプト", "jsx"], &["js", "jsx"]),
        (&["markdown", "マークダウン"], &["md"]),
        (&["csv"], &["csv"]),
        (&["json"], &["json"]),
        (&["toml"], &["toml"]),
        (&["yaml", "yml"], &["yaml", "yml"]),
        (&["zip", "アーカイブ"], &["zip"]),
        (&["画像", "image", "img"], &["png", "jpg", "jpeg", "gif", "webp"]),
        (&["動画", "video", "movie"], &["mp4", "mov", "avi", "mkv"]),
        (&["音楽", "音声", "audio"], &["mp3", "wav", "flac", "aac"]),
        (&["テキスト", "text file"], &["txt"]),
    ];

    for (keywords, mapped_exts) in aliases {
        if keywords.iter().any(|k| lower.contains(k)) {
            for e in *mapped_exts {
                if !exts.contains(&e.to_string()) {
                    exts.push(e.to_string());
                }
            }
        }
    }

    exts
}

fn extract_path(input: &str) -> Option<String> {
    // 直接構文: path:src/
    let lower = input.to_lowercase();
    if let Some(idx) = lower.find("path:") {
        let rest = &input[idx + 5..];
        let value = rest.split_whitespace().next().unwrap_or("");
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    // src/ のようなパス表記（スラッシュを含むトークン）
    // スラッシュまでの部分だけをパスとして切り出す。
    // 例: "src/のTypeScript" → "src/"
    for token in input.split_whitespace() {
        if token.starts_with("http") {
            continue;
        }
        if let Some(slash_pos) = token.find('/') {
            // スラッシュの直後が ASCII 英数字やパス文字ならディレクトリ指定とみなす
            let after_slash = &token[slash_pos + 1..];
            let is_path = after_slash.is_empty()
                || after_slash
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
                    .unwrap_or(false);
            if is_path {
                // スラッシュ以降も有効なパス文字が続く間だけ取る
                let path_end = slash_pos
                    + 1
                    + after_slash
                        .find(|c: char| {
                            !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | '.' | '/')
                        })
                        .unwrap_or(after_slash.len());
                return Some(token[..path_end].to_string());
            }
        }
    }

    None
}

fn is_extension_alias(t: &str) -> bool {
    matches!(
        t,
        "pdf" | "excel" | "エクセル" | "xlsx" | "xls"
        | "word" | "ワード" | "docx" | "doc"
        | "powerpoint" | "パワーポイント" | "pptx" | "ppt"
        | "python" | "パイソン" | "rust" | "ラスト"
        | "typescript" | "タイプスクリプト" | "tsx"
        | "javascript" | "ジャバスクリプト" | "jsx"
        | "markdown" | "マークダウン"
        | "csv" | "json" | "toml" | "yaml" | "yml"
        | "zip" | "アーカイブ"
        | "画像" | "image" | "img"
        | "動画" | "video" | "movie"
        | "音楽" | "音声" | "audio"
        | "テキスト"
    )
}

fn is_date_keyword(t: &str) -> bool {
    matches!(
        t,
        "今日" | "本日" | "today"
        | "昨日" | "yesterday"
        | "今週" | "先週" | "lastweek"
        | "今月" | "先月"
        | "今年" | "去年" | "昨年"
    )
}

/// 日付・サイズ・拡張子ルールが消費するノイズトークンを除外する。
fn is_noise_token(t: &str) -> bool {
    matches!(
        t,
        "今日" | "昨日" | "今週" | "先週" | "今月" | "先月" | "今年" | "去年" | "昨年"
        | "以上" | "以下" | "より大きい" | "より小さい" | "超" | "未満"
        | "pdf" | "excel" | "エクセル" | "word" | "ワード"
        | "python" | "rust" | "typescript" | "javascript"
        | "画像" | "動画" | "音楽" | "音声"
        | "の" | "な" | "で" | "が" | "を" | "は" | "に" | "と" | "も"
        | "ファイル" | "file" | "files"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_today() {
        let q = parse_query("今日編集したファイル");
        assert_eq!(q.date_modified, Some("today".to_string()));
    }

    #[test]
    fn test_date_lastweek() {
        let q = parse_query("先週保存したExcelファイル");
        assert_eq!(q.date_modified, Some("lastweek".to_string()));
        assert!(q.extensions.contains(&"xlsx".to_string()));
    }

    #[test]
    fn test_ext_pdf() {
        let q = parse_query("大事なPDFを探して");
        assert!(q.extensions.contains(&"pdf".to_string()));
    }

    #[test]
    fn test_ext_typescript() {
        let q = parse_query("srcフォルダのTypeScriptファイル");
        assert!(q.extensions.iter().any(|e| e == "ts" || e == "tsx"));
    }

    #[test]
    fn test_path_filter() {
        let q = parse_query("src/components のファイル");
        assert_eq!(q.path_filter, Some("src/components".to_string()));
    }

    #[test]
    fn test_size_min() {
        let q = parse_query("10MB以上のファイル");
        assert_eq!(q.min_size, Some(10 * (1 << 20)));
    }

    #[test]
    fn test_everything_query_combined() {
        let q = ParsedQuery {
            keywords: vec!["report".to_string()],
            extensions: vec!["pdf".to_string()],
            path_filter: None,
            min_size: Some(1 << 20),
            max_size: None,
            date_modified: Some("lastweek".to_string()),
        };
        let s = q.to_everything_query();
        assert!(s.contains("ext:pdf"));
        assert!(s.contains("dm:lastweek"));
        assert!(s.contains("size:>"));
        // "report"(6文字) は前半3文字に打ち切られて "rep" になる
        assert!(s.contains("rep"));
    }

    #[test]
    fn test_everything_term_truncates_long_keyword() {
        let q = ParsedQuery {
            keywords: vec!["reprot".to_string()],
            ..Default::default()
        };
        let s = q.to_everything_query();
        // 6文字 → 前半3文字 "rep" に打ち切られる
        assert_eq!(s, "rep");
    }

    #[test]
    fn test_everything_term_keeps_short_keyword() {
        let q = ParsedQuery {
            keywords: vec!["foo".to_string()],
            ..Default::default()
        };
        let s = q.to_everything_query();
        assert_eq!(s, "foo");
    }

    #[test]
    fn test_everything_term_transposition_typo_survives_filter() {
        // "report" の隣接転置タイポ "reprot" を打ち切ると "rep" になり、
        // 正しいファイル名 "report" の部分文字列として残るため
        // Everything の絞り込みを通過できる（Phase1候補に入る）。
        let q = ParsedQuery {
            keywords: vec!["reprot".to_string()],
            ..Default::default()
        };
        let term = q.to_everything_query();
        assert!("report".contains(&term), "term={term} should be substring of report");
    }

    #[test]
    fn test_direct_syntax_ext() {
        let q = parse_query("ext:rs,toml path:src/ modified:lastweek");
        assert!(q.extensions.contains(&"rs".to_string()));
        assert!(q.extensions.contains(&"toml".to_string()));
        assert!(q.path_filter.is_some());
    }
}
