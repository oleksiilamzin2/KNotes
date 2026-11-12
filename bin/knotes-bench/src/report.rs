//! Budget table of ADR 0006 and the verdict for each measured operation.

use std::fmt::Write as _;
use std::time::Duration;

pub struct Row {
    pub operation: String,
    pub measured: Duration,
    /// Budget on an M1; `None` for operations reported without a budget.
    pub budget: Option<Duration>,
}

/// Markdown table; the M1 estimate is the measurement times `m1_factor`.
pub fn render(rows: &[Row], m1_factor: f64) -> String {
    let mut out = String::from("| Operation | Measured | M1 estimate | M1 budget | Verdict |\n|---|---:|---:|---:|---|\n");
    for row in rows {
        let estimate = row.measured.mul_f64(m1_factor);
        let (budget, verdict) = match row.budget {
            Some(b) if estimate <= b => (fmt(b), "ok"),
            Some(b) => (fmt(b), "**over**"),
            None => ("—".to_owned(), ""),
        };
        let _ = writeln!(out, "| {} | {} | {} | {budget} | {verdict} |", row.operation, fmt(row.measured), fmt(estimate));
    }
    out
}

fn fmt(d: Duration) -> String {
    match d.as_secs_f64() {
        s if s >= 1.0 => format!("{s:.2} s"),
        s if s >= 0.001 => format!("{:.2} ms", s * 1e3),
        s => format!("{:.0} µs", s * 1e6),
    }
}

/// The value at quantile `q` (0..=1) of unsorted samples.
pub fn quantile(samples: &mut [Duration], q: f64) -> Duration {
    samples.sort_unstable();
    samples[((samples.len() - 1) as f64 * q).round() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_uses_the_m1_estimate() {
        let rows = [
            Row { operation: "fast".into(), measured: Duration::from_millis(2), budget: Some(Duration::from_millis(5)) },
            Row { operation: "slow".into(), measured: Duration::from_millis(3), budget: Some(Duration::from_millis(5)) },
            Row { operation: "info".into(), measured: Duration::from_secs(2), budget: None },
        ];
        let table = render(&rows, 2.0);
        assert!(table.contains("| fast | 2.00 ms | 4.00 ms | 5.00 ms | ok |"), "{table}");
        assert!(table.contains("| slow | 3.00 ms | 6.00 ms | 5.00 ms | **over** |"), "{table}");
        assert!(table.contains("| info | 2.00 s | 4.00 s | — |  |"), "{table}");
    }

    #[test]
    fn quantiles() {
        let mut s: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(quantile(&mut s, 0.5), Duration::from_millis(51));
        assert_eq!(quantile(&mut s, 0.95), Duration::from_millis(95));
    }
}
