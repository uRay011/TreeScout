use embedding::{cosine_f32, cosine_i8, quantize_f32_to_i8};

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().map(|x| x / norm).collect()
}

fn demo(label: &str, a: &[f32], b: &[f32]) {
    let a = l2_normalize(a);
    let b = l2_normalize(b);
    let f32_score = cosine_f32(&a, &b);
    let qa = quantize_f32_to_i8(&a);
    let qb = quantize_f32_to_i8(&b);
    let i8_score = cosine_i8(&qa, &qb);
    println!(
        "{:<30}  f32={:+.4}  i8={:+.4}  err={:.4}",
        label,
        f32_score,
        i8_score,
        (f32_score - i8_score).abs()
    );
}

fn main() {
    println!("{:<30}  {:>10}  {:>10}  {:>8}", "ペア", "cosine_f32", "cosine_i8", "誤差");
    println!("{}", "-".repeat(64));

    // 同一ベクトル → 1.0
    let v = vec![1.0f32, 0.0, 0.0, 0.0];
    demo("同一ベクトル", &v, &v);

    // 直交 → 0.0
    demo("直交", &[1.0, 0.0, 0.0, 0.0], &[0.0, 1.0, 0.0, 0.0]);

    // 反対 → -1.0
    demo("反対方向", &[1.0, 0.0], &[-1.0, 0.0]);

    // 近い（cos ≈ 0.98）
    demo("近い (45°の半分)", &[1.0, 0.2, 0.0, 0.0], &[1.0, 0.3, 0.0, 0.0]);

    // 45度
    demo("45度", &[1.0, 0.0], &[1.0, 1.0]);

    // 384次元ランダム（疑似的な埋め込みベクトルの想定）
    let mut state = 42u64;
    let mut next = || -> f32 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) as f32 / (u32::MAX as f32)) * 2.0 - 1.0
    };
    let rand_a: Vec<f32> = (0..384).map(|_| next()).collect();
    let rand_b: Vec<f32> = (0..384).map(|_| next()).collect();
    demo("384次元ランダム", &rand_a, &rand_b);

    // 384次元・似た方向
    let base: Vec<f32> = (0..384).map(|i| (i as f32 * 0.1).sin()).collect();
    let perturbed: Vec<f32> = base.iter().enumerate().map(|(i, x)| x + if i < 10 { 0.05 } else { 0.0 }).collect();
    demo("384次元・近い方向", &base, &perturbed);
}
