/// How wide a kept vector is: the first 512 of the model's (e5's 1024, EmbeddingGemma 2's 768).
/// On the benchmark e5 found 63 of 65 at 512 and 64 at 1024, for half the memory (ADR 0029).
pub const DIMS: usize = 512;

/// A vector normalized and scaled to bytes (×127).
pub type Vector = [i8; DIMS];

/// The model's vector cut to its first `DIMS`, normalized and kept in bytes.
pub fn quantize(full: &[f32]) -> Vector {
    let cut = &full[..full.len().min(DIMS)];
    let norm = cut.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mut v = [0i8; DIMS];
    if norm > 0.0 {
        for (out, x) in v.iter_mut().zip(cut) {
            *out = (x / norm * 127.0).round().clamp(-127.0, 127.0) as i8;
        }
    }
    v
}

/// How alike two vectors are: their dot product, higher the nearer (127² for one with itself).
pub fn similarity(a: &Vector, b: &Vector) -> i32 {
    a.iter().zip(b).map(|(x, y)| i32::from(*x) * i32::from(*y)).sum()
}
