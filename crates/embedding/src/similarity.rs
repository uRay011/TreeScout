/// 正規化済み float32 ベクトルのコサイン類似度（= 内積）
///
/// 両ベクトルが L2 正規化済みであることを前提とする。
/// model2vec-rs の `StaticModel` は `normalize=true` で出力を正規化するため満たされる。
#[inline]
pub fn cosine_f32(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "次元数不一致");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// int8 量子化ベクトルのコサイン類似度（正規化済み前提）
///
/// SIMD 最適化はコンパイラの自動ベクタライズに委ねる（target-feature = avx2 を想定）。
#[inline]
pub fn cosine_i8(a: &[i8], b: &[i8]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "次元数不一致");
    let dot: i32 = a.iter().zip(b.iter()).map(|(&x, &y)| x as i32 * y as i32).sum();
    // int8 正規化定数: (127.0)^2 = 16129
    dot as f32 / 16129.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn cosine_identical_f32() {
        let v = vec![1.0f32 / 2f32.sqrt(), 1.0 / 2f32.sqrt()];
        assert_abs_diff_eq!(cosine_f32(&v, &v), 1.0, epsilon = 1e-6);
    }

    #[test]
    fn cosine_orthogonal_f32() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert_abs_diff_eq!(cosine_f32(&a, &b), 0.0, epsilon = 1e-6);
    }

    #[test]
    fn cosine_i8_identical() {
        let v = vec![90i8, 90];
        let sim = cosine_i8(&v, &v);
        assert!(sim > 0.9, "sim={sim}");
    }

    // --- end-to-end 精度検証 ---

    fn l2_normalize(v: &[f32]) -> Vec<f32> {
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / norm).collect()
    }

    /// quantize → cosine_i8 vs 素の cosine_f32 の誤差が A* ヒューリスティックとして許容範囲か確認
    ///
    /// 384次元のランダムベクトル（L2正規化済み）で誤差を計測する。
    /// 目標: |cosine_f32 - cosine_i8_via_quantize| < 0.02
    #[test]
    fn e2e_cosine_error_within_tolerance() {
        use crate::quantize::quantize_f32_to_i8;

        // 再現性のある擬似乱数（LCG）で 384 次元ベクトルを生成
        let mut state = 12345u64;
        let mut next = || -> f32 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            // [-1, 1] に正規化
            ((state >> 33) as f32 / (u32::MAX as f32)) * 2.0 - 1.0
        };

        let raw_a: Vec<f32> = (0..384).map(|_| next()).collect();
        let raw_b: Vec<f32> = (0..384).map(|_| next()).collect();
        let a = l2_normalize(&raw_a);
        let b = l2_normalize(&raw_b);

        let expected = cosine_f32(&a, &b);

        let qa = quantize_f32_to_i8(&a);
        let qb = quantize_f32_to_i8(&b);
        let actual = cosine_i8(&qa, &qb);

        let err = (expected - actual).abs();
        println!("cosine_f32={expected:.6}  cosine_i8={actual:.6}  err={err:.6}");
        assert!(err < 0.02, "誤差 {err:.6} が許容値 0.02 を超えた");
    }

    /// 類似ベクトル（高スコア）・直交ベクトル（低スコア）で
    /// int8 量子化後も大小関係が保たれるか（A* ランキングの正確性）
    #[test]
    fn e2e_ranking_order_preserved() {
        use crate::quantize::quantize_f32_to_i8;

        // query: [1, 0, 0, ...]
        let dim = 64usize;
        let query: Vec<f32> = (0..dim).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();

        // similar: query に近い（cos ≈ 0.99）
        let mut similar = query.clone();
        similar[1] = 0.1;
        let similar = l2_normalize(&similar);

        // dissimilar: ほぼ直交
        let dissimilar: Vec<f32> = (0..dim).map(|i| if i == 1 { 1.0 } else { 0.0 }).collect();

        let q_query = quantize_f32_to_i8(&query);
        let q_similar = quantize_f32_to_i8(&similar);
        let q_dissimilar = quantize_f32_to_i8(&dissimilar);

        let score_similar = cosine_i8(&q_query, &q_similar);
        let score_dissimilar = cosine_i8(&q_query, &q_dissimilar);

        println!("score_similar={score_similar:.4}  score_dissimilar={score_dissimilar:.4}");
        assert!(
            score_similar > score_dissimilar,
            "ランキング逆転: similar={score_similar:.4} <= dissimilar={score_dissimilar:.4}"
        );
    }

    /// 384次元・1000ペアの最大誤差と平均誤差をレポート
    #[test]
    fn e2e_error_statistics_384dim_1000pairs() {
        use crate::quantize::quantize_f32_to_i8;

        let mut state = 99999u64;
        let mut next = || -> f32 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) as f32 / (u32::MAX as f32)) * 2.0 - 1.0
        };

        let n = 1000usize;
        let dim = 384usize;
        let mut max_err = 0.0f32;
        let mut sum_err = 0.0f32;

        for _ in 0..n {
            let raw_a: Vec<f32> = (0..dim).map(|_| next()).collect();
            let raw_b: Vec<f32> = (0..dim).map(|_| next()).collect();
            let a = l2_normalize(&raw_a);
            let b = l2_normalize(&raw_b);

            let f = cosine_f32(&a, &b);
            let qa = quantize_f32_to_i8(&a);
            let qb = quantize_f32_to_i8(&b);
            let i = cosine_i8(&qa, &qb);

            let err = (f - i).abs();
            sum_err += err;
            if err > max_err { max_err = err; }
        }

        let mean_err = sum_err / n as f32;
        println!("1000ペア統計: max_err={max_err:.6}  mean_err={mean_err:.6}");
        assert!(max_err < 0.02, "最大誤差 {max_err:.6} が 0.02 を超えた");
        assert!(mean_err < 0.005, "平均誤差 {mean_err:.6} が 0.005 を超えた");
    }
}
