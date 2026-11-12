use rand::RngExt;

use crate::Rng;

/// Zipf–Mandelbrot distribution over ranks `0..n`: weight of rank `r` is `1 / (r + q)^s`.
pub(crate) struct Zipf {
    cdf: Vec<f64>,
}

impl Zipf {
    pub fn new(n: usize, s: f64, q: f64) -> Self {
        let mut acc = 0.0;
        let cdf = (0..n.max(1))
            .map(|r| {
                acc += 1.0 / (r as f64 + q).powf(s);
                acc
            })
            .collect();
        Self { cdf }
    }

    pub fn sample(&self, rng: &mut Rng) -> usize {
        let u = rng.random::<f64>() * self.cdf[self.cdf.len() - 1];
        self.cdf.partition_point(|&c| c < u).min(self.cdf.len() - 1)
    }
}
