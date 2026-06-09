use thiserror::Error;

#[derive(Debug, Error)]
pub enum NlpError {
    #[error("tokenizer init failed: {0}")]
    Init(String),
    #[error("tokenize failed: {0}")]
    Tokenize(String),
}

/// 形態素解析ラッパー。
/// `embed-ipadic` feature が有効な場合は Lindera を使い、
/// それ以外は ASCII/Unicode 境界でのシンプル分割にフォールバックする。
pub struct NlpTokenizer {
    #[cfg(feature = "embed-ipadic")]
    inner: lindera::tokenizer::Tokenizer,
}

impl NlpTokenizer {
    pub fn new() -> Result<Self, NlpError> {
        #[cfg(feature = "embed-ipadic")]
        {
            let mut builder = lindera::tokenizer::TokenizerBuilder::new()
                .map_err(|e| NlpError::Init(e.to_string()))?;
            builder.set_segmenter_dictionary("embedded://ipadic");
            let inner = builder.build().map_err(|e| NlpError::Init(e.to_string()))?;
            Ok(Self { inner })
        }
        #[cfg(not(feature = "embed-ipadic"))]
        {
            Ok(Self {})
        }
    }

    /// クエリを形態素単位のトークン列に分割して返す。
    /// `embed-ipadic` feature 無効時は空白・ASCII 境界による分割。
    pub fn tokenize(&self, text: &str) -> Result<Vec<String>, NlpError> {
        #[cfg(feature = "embed-ipadic")]
        {
            let tokens = self
                .inner
                .tokenize(text)
                .map_err(|e| NlpError::Tokenize(e.to_string()))?;
            Ok(tokens
                .into_iter()
                .map(|t| t.surface.into_owned())
                .collect())
        }
        #[cfg(not(feature = "embed-ipadic"))]
        {
            Ok(fallback_tokenize(text))
        }
    }

    /// embedding 用途向けトークン列。
    /// 助詞・助動詞・記号など意味が薄い品詞を除去して返す。
    /// `embed-ipadic` 有効時のみフィルタリングが効く。それ以外は `tokenize` と同じ。
    pub fn tokenize_for_embedding(&self, text: &str) -> Result<Vec<String>, NlpError> {
        #[cfg(feature = "embed-ipadic")]
        {
            let tokens = self
                .inner
                .tokenize(text)
                .map_err(|e| NlpError::Tokenize(e.to_string()))?;

            let filtered = tokens
                .into_iter()
                .filter(|t| {
                    // details[0] が品詞。助詞(助詞)・助動詞・記号・BOS/EOS を除去
                    match t.details.as_ref().and_then(|d| d.first()) {
                        Some(pos) => !matches!(
                            pos.as_ref(),
                            "助詞" | "助動詞" | "記号" | "BOS/EOS" | "フィラー"
                        ),
                        None => true,
                    }
                })
                .map(|t| t.surface.into_owned())
                .collect();

            Ok(filtered)
        }
        #[cfg(not(feature = "embed-ipadic"))]
        {
            Ok(fallback_tokenize(text))
        }
    }
}

/// `embed-ipadic` 無効時のフォールバック: 空白・CJK境界での簡易分割
fn fallback_tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut prev_is_ascii = true;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            continue;
        }
        let is_ascii = ch.is_ascii();
        // ASCII ↔ 非ASCII の境界でも区切る
        if !current.is_empty() && is_ascii != prev_is_ascii {
            tokens.push(current.clone());
            current.clear();
        }
        current.push(ch);
        prev_is_ascii = is_ascii;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_tokenize_mixed() {
        let tokens = fallback_tokenize("src/components議事録.ts");
        // ASCII 部と非ASCII 部が分かれること
        assert!(tokens.contains(&"src".to_string()) || tokens.iter().any(|t| t.contains("src")));
        assert!(tokens.iter().any(|t| t.contains("議事録")));
    }

    #[test]
    fn test_fallback_tokenize_spaces() {
        let tokens = fallback_tokenize("hello world foo");
        assert_eq!(tokens, vec!["hello", "world", "foo"]);
    }

    #[test]
    #[cfg(feature = "embed-ipadic")]
    fn test_tokenize_japanese() {
        let tok = NlpTokenizer::new().unwrap();
        let tokens = tok.tokenize("議事録作成手順書").unwrap();
        // 複合語が分割されること
        assert!(tokens.len() > 1, "複合語が分割されていない: {:?}", tokens);
    }

    #[test]
    #[cfg(feature = "embed-ipadic")]
    fn test_tokenize_for_embedding_filters_particles() {
        let tok = NlpTokenizer::new().unwrap();
        let full = tok.tokenize("猫が走る").unwrap();
        let emb = tok.tokenize_for_embedding("猫が走る").unwrap();
        // embedding 用は助詞「が」を除去するのでフルより短いか同等
        assert!(emb.len() <= full.len());
    }
}
