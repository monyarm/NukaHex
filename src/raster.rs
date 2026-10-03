//! Small raster helpers shared by the art-reading extractors.

/// Box mean of `v` (w x h) over a (2r+1)^2 window, clamped at the edges, via an integral image.
pub fn box_mean(v: &[f64], w: usize, h: usize, r: usize) -> Vec<f64> {
    let mut sat = vec![0.0; (w + 1) * (h + 1)];
    for y in 0..h { for x in 0..w {
        sat[(y + 1) * (w + 1) + x + 1] = v[y * w + x] + sat[y * (w + 1) + x + 1] + sat[(y + 1) * (w + 1) + x] - sat[y * (w + 1) + x];
    }}
    let mut out = vec![0.0; w * h];
    for y in 0..h { for x in 0..w {
        let (x0, y0, x1, y1) = (x.saturating_sub(r), y.saturating_sub(r), (x + r + 1).min(w), (y + r + 1).min(h));
        let s = sat[y1 * (w + 1) + x1] - sat[y0 * (w + 1) + x1] - sat[y1 * (w + 1) + x0] + sat[y0 * (w + 1) + x0];
        out[y * w + x] = s / ((x1 - x0) * (y1 - y0)) as f64;
    }}
    out
}
