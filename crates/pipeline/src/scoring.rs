//! 段階的スコアラー（キーワードベース暫定実装）
//!
//! Phase 4 でフォルダ/ファイル embedding（cosine類似度）に置換予定。
//! 現時点では「完全一致=1.0、部分一致・祖先一致を段階値」で近似し、
//! 単一キーワードでも候補スコアが分散しヒートマップが段階変化するようにする。
//!
//! Tauri 非依存のため `cargo test -p pipeline` でクロスプラットフォーム検証可能。

use std::path::Path;

/// パスのスコアを [0, 1] で返す。
///
/// - `keywords` 空 → 0.0（フィルタなし時は無着色）
/// - ファイル名（拡張子除く stem）が keyword と完全一致 → 1.0
/// - ファイル名が keyword を部分一致 → 前方一致ボーナス＋被覆率で 0.6〜0.95
/// - ファイル名に無く祖先パス成分が含む → 中間点（距離で減衰）
/// - どれにも一致しない → 0.1
/// - `extensions` 指定時、拡張子一致で +0.2（上限1.0）
pub fn score_path(path: &Path, keywords: &[String], extensions: &[String]) -> f32 {
    if keywords.is_empty() {
        return 0.0;
    }

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| file_name.clone());

    // 祖先パス成分（自身を除く）を小文字で集める
    let ancestors: Vec<String> = path
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str())
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default();

    // キーワードごとにスコアを算出し平均する
    let total: f32 = keywords
        .iter()
        .map(|kw| score_one(kw, &file_name, &stem, &ancestors))
        .sum();
    let base = total / keywords.len() as f32;

    // 拡張子ボーナス
    let ext_bonus = if !extensions.is_empty() {
        let file_ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if extensions.iter().any(|e| e.to_lowercase() == file_ext) {
            0.2
        } else {
            0.0
        }
    } else {
        0.0
    };

    (base + ext_bonus).min(1.0)
}

/// 単一キーワードの一致度を [0.1, 1.0] で返す。
fn score_one(kw: &str, file_name: &str, stem: &str, ancestors: &[String]) -> f32 {
    if kw.is_empty() {
        return 0.1;
    }

    // 1) stem 完全一致 → 1.0
    if stem == kw {
        return 1.0;
    }

    // 2) ファイル名が部分一致 → 前方一致ボーナス + 被覆率
    if let Some(pos) = file_name.find(kw) {
        // 被覆率: keyword 長 / ファイル名長（短い名前ほど高スコア）
        let coverage = if file_name.is_empty() {
            0.0
        } else {
            kw.chars().count() as f32 / file_name.chars().count() as f32
        };
        let prefix_bonus = if pos == 0 { 0.1 } else { 0.0 };
        return (0.6 + 0.25 * coverage + prefix_bonus).min(0.95);
    }

    // 3) 祖先パス成分が含む → 中間点。自身に近い（末尾側＝深い）祖先ほど高く減衰させる
    for (depth_from_self, comp) in ancestors.iter().rev().enumerate() {
        if comp.contains(kw) {
            // depth_from_self=0（直近の親）→ 0.5、以降 0.05 ずつ減衰、下限 0.35
            let decayed = 0.5 - 0.05 * depth_from_self as f32;
            return decayed.max(0.35);
        }
    }

    // 4) 非一致
    0.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn kw(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn exact_stem_match_is_one() {
        let p = PathBuf::from("/a/b/昇格.pdf");
        let s = score_path(&p, &kw(&["昇格"]), &[]);
        approx::assert_relative_eq!(s, 1.0, epsilon = 1e-6);
    }

    #[test]
    fn partial_name_match_below_one() {
        let p = PathBuf::from("/a/b/2020年昇格試験のご案内.xlsx");
        let s = score_path(&p, &kw(&["昇格"]), &[]);
        assert!(s > 0.1 && s < 1.0, "got {s}");
    }

    #[test]
    fn shorter_name_scores_higher() {
        let short = PathBuf::from("/a/昇格試験.docx");
        let long = PathBuf::from("/a/2020年度第3四半期昇格試験運用FAQ詳細版.docx");
        let ss = score_path(&short, &kw(&["昇格"]), &[]);
        let sl = score_path(&long, &kw(&["昇格"]), &[]);
        assert!(ss > sl, "short {ss} should beat long {sl}");
    }

    #[test]
    fn ancestor_only_match_is_mid() {
        // ファイル名に「昇格」を含まないが親フォルダが含む
        let p = PathBuf::from("/users/me/2020年昇格試験のご案内/論文.docx");
        let s = score_path(&p, &kw(&["昇格"]), &[]);
        assert!(s > 0.1 && s < 0.6, "ancestor-only should be mid, got {s}");
    }

    #[test]
    fn no_match_is_floor() {
        let p = PathBuf::from("/a/b/readme.txt");
        let s = score_path(&p, &kw(&["昇格"]), &[]);
        approx::assert_relative_eq!(s, 0.1, epsilon = 1e-6);
    }

    #[test]
    fn empty_keywords_is_zero() {
        let p = PathBuf::from("/a/b/昇格.pdf");
        let s = score_path(&p, &[], &[]);
        approx::assert_relative_eq!(s, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn extension_bonus_applies() {
        // 部分一致（base < 1.0）でボーナスの効果を確認する
        let p = PathBuf::from("/a/b/会議資料2024.pdf");
        let without = score_path(&p, &kw(&["資料"]), &[]);
        let with = score_path(&p, &kw(&["資料"]), &kw(&["pdf"]));
        assert!(with > without, "ext bonus should raise score: {with} vs {without}");
        assert!(with <= 1.0);
    }

    #[test]
    fn deeper_ancestor_decays() {
        // 直近の親に一致する方が、遠い祖先に一致するより高い
        let near = PathBuf::from("/a/昇格フォルダ/file.txt");
        let far = PathBuf::from("/a/昇格フォルダ/x/y/z/file.txt");
        let sn = score_path(&near, &kw(&["昇格"]), &[]);
        let sf = score_path(&far, &kw(&["昇格"]), &[]);
        assert!(sn >= sf, "near {sn} should be >= far {sf}");
    }
}
