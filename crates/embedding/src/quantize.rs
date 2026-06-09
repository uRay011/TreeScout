/// float32 ベクトルを int8 に量子化する（[-1, 1] → [-127, 127]）
///
/// model2vec-rs が L2 正規化済み出力を返す場合、各要素は [-1, 1] に収まる。
/// スケールを 127.0 固定にすることでデシリアライズ時の per-vector スケール保存を省く。
pub fn quantize_f32_to_i8(v: &[f32]) -> Vec<i8> {
    v.iter()
        .map(|&x| (x.clamp(-1.0, 1.0) * 127.0).round() as i8)
        .collect()
}

/// int8 → float32 復元
pub fn dequantize_i8(v: &[i8]) -> Vec<f32> {
    v.iter().map(|&x| x as f32 / 127.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn roundtrip() {
        let original = vec![0.5f32, -0.5, 0.0, 1.0, -1.0];
        let q = quantize_f32_to_i8(&original);
        let r = dequantize_i8(&q);
        for (a, b) in original.iter().zip(r.iter()) {
            // int8 の量子化誤差は最大 1/127 ≈ 0.008
            assert_abs_diff_eq!(a, b, epsilon = 0.009);
        }
    }
}
