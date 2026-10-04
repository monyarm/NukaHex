// Rubbersheeting with a single thin-plate spline (TPS) per game.
//
// Forward (game -> lat/lon) is the TPS itself (affine part + bending part).
// Inverse (lat/lon -> game) is NOT fitted separately: it is solved numerically from the
// forward map (Newton), so markers (forward) and terrain sampling (inverse) always agree.
//
// Anchors are interpolated exactly. A warp that hits every anchor exactly must fold if
// the game and the real world disagree on the winding of any anchor triangle, so
// `resolve_orientation_conflicts` first moves anchors minimally to remove such conflicts.
use log::warn;

#[derive(Debug, Clone)]
pub struct ControlPoint {
    pub name: String,
    pub game_x: f32,
    pub game_y: f32,
    pub real_lat: f32,
    pub real_lon: f32,
}

pub struct RubberSheet {
    // Control points in normalized game coordinates.
    pts: Vec<(f64, f64)>,
    center: (f64, f64),
    scale: f64,
    // Per output (lat, lon): kernel weights and affine (a0, ax, ay).
    w: [Vec<f64>; 2],
    a: [[f64; 3]; 2],
    valid: bool,
    // Forward samples over the game domain, binned by (lat, lon): (bin size, bin -> [(lat, lon, u, v)]).
    // Lets the inverse reject points outside the map at once and start Newton next to the answer.
    domain: Option<(f64, std::collections::HashMap<(i64, i64), Vec<(f64, f64, f64, f64)>>)>,
}

fn kernel(r2: f64) -> f64 {
    if r2 < 1e-20 { 0.0 } else { 0.5 * r2 * r2.ln() } // r^2 ln r == 0.5 r^2 ln r^2
}

/// Gaussian elimination with partial pivoting, solving A x = b for several right-hand sides.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<[f64; 2]>) -> Option<Vec<[f64; 2]>> {
    let n = a.len();
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-12 { return None; }
        a.swap(col, piv);
        b.swap(col, piv);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            if f == 0.0 { continue; }
            for k in col..n { a[row][k] -= f * a[col][k]; }
            for k in 0..2 { b[row][k] -= f * b[col][k]; }
        }
    }
    let mut x = vec![[0.0; 2]; n];
    for row in (0..n).rev() {
        for k in 0..2 {
            let s: f64 = (row + 1..n).map(|j| a[row][j] * x[j][k]).sum();
            x[row][k] = (b[row][k] - s) / a[row][row];
        }
    }
    Some(x)
}

impl RubberSheet {
    pub fn new(control_points: Vec<ControlPoint>) -> Self {
        Self::smoothed(control_points, 0.0)
    }

    /// A TPS that fits the anchors approximately: `smoothing` (lambda, in units of the normalized
    /// kernel) trades exactness at the anchors for a gentler warp; 0 interpolates them exactly, and
    /// as it grows the warp tends to the anchors' least-squares affine fit.
    pub fn smoothed(control_points: Vec<ControlPoint>, smoothing: f64) -> Self {
        let n = control_points.len();
        let invalid = RubberSheet { pts: vec![], center: (0.0, 0.0), scale: 1.0, w: [vec![], vec![]], a: [[0.0; 3]; 2], valid: false, domain: None };
        if n < 3 { return invalid; }

        let cx = control_points.iter().map(|c| c.game_x as f64).sum::<f64>() / n as f64;
        let cy = control_points.iter().map(|c| c.game_y as f64).sum::<f64>() / n as f64;
        let scale = (control_points.iter()
            .map(|c| (c.game_x as f64 - cx).powi(2) + (c.game_y as f64 - cy).powi(2))
            .sum::<f64>() / n as f64).sqrt().max(1e-9);
        let pts: Vec<(f64, f64)> = control_points.iter()
            .map(|c| ((c.game_x as f64 - cx) / scale, (c.game_y as f64 - cy) / scale))
            .collect();

        // [K + Lambda  P] [w]   [v]
        // [P^T         0] [a] = [0]
        let m = n + 3;
        let mut l = vec![vec![0.0; m]; m];
        let mut rhs = vec![[0.0; 2]; m];
        for i in 0..n {
            for j in 0..n {
                let (dx, dy) = (pts[i].0 - pts[j].0, pts[i].1 - pts[j].1);
                l[i][j] = kernel(dx * dx + dy * dy) + if i == j { smoothing } else { 0.0 };
            }
            let p = [1.0, pts[i].0, pts[i].1];
            for k in 0..3 { l[i][n + k] = p[k]; l[n + k][i] = p[k]; }
            rhs[i] = [control_points[i].real_lat as f64, control_points[i].real_lon as f64];
        }

        let Some(x) = solve(l, rhs) else {
            warn!("[Rubbersheeting] Singular TPS system (collinear or duplicate control points)");
            return invalid;
        };
        let w = [x[..n].iter().map(|v| v[0]).collect(), x[..n].iter().map(|v| v[1]).collect()];
        let a = [[x[n][0], x[n + 1][0], x[n + 2][0]], [x[n][1], x[n + 1][1], x[n + 2][1]]];
        RubberSheet { pts, center: (cx, cy), scale, w, a, valid: true, domain: None }
    }

    fn eval_normalized(&self, u: f64, v: f64) -> (f64, f64) {
        let mut out = [0.0; 2];
        for k in 0..2 {
            out[k] = self.a[k][0] + self.a[k][1] * u + self.a[k][2] * v;
        }
        for (i, p) in self.pts.iter().enumerate() {
            let (dx, dy) = (u - p.0, v - p.1);
            let k = kernel(dx * dx + dy * dy);
            out[0] += self.w[0][i] * k;
            out[1] += self.w[1][i] * k;
        }
        (out[0], out[1])
    }

    /// Game -> Real (lat, lon).
    pub fn transform(&self, game_x: f32, game_y: f32) -> (f32, f32) {
        if !self.valid { return (0.0, 0.0); }
        let u = (game_x as f64 - self.center.0) / self.scale;
        let v = (game_y as f64 - self.center.1) / self.scale;
        let (lat, lon) = self.eval_normalized(u, v);
        (lat as f32, lon as f32)
    }

    /// Real (lat, lon) -> Game, solved from the forward map so it is its exact inverse.
    /// Returns None where Newton does not converge (outside the map's invertible region).
    pub fn inverse_transform(&self, lat: f32, lon: f32) -> Option<(f32, f32)> {
        if !self.valid { return None; }
        let (lat, lon) = (lat as f64, lon as f64);

        if let Some((bin, index)) = &self.domain {
            // Nearest forward sample in the 3x3 bins around the query; none = outside the map.
            let (by, bx) = ((lat / bin).floor() as i64, (lon / bin).floor() as i64);
            let start = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (by + dy, bx + dx)))
                .filter_map(|k| index.get(&k)).flatten()
                .min_by(|a, b| ((a.0 - lat).powi(2) + (a.1 - lon).powi(2)).total_cmp(&((b.0 - lat).powi(2) + (b.1 - lon).powi(2))))?;
            return self.newton((start.2, start.3), lat, lon)
                .map(|(u, v)| ((u * self.scale + self.center.0) as f32, (v * self.scale + self.center.1) as f32));
        }

        // Start points: the affine inverse, then every anchor (nearest in real space first).
        // Plain Newton from the affine guess alone can fail where the warp is far from affine.
        let mut starts = Vec::with_capacity(self.pts.len() + 1);
        let [[a0, a1, a2], [b0, b1, b2]] = self.a;
        let det = a1 * b2 - a2 * b1;
        if det.abs() > 1e-15 {
            starts.push((((lat - a0) * b2 - a2 * (lon - b0)) / det, (a1 * (lon - b0) - (lat - a0) * b1) / det));
        }
        let mut anchors: Vec<(f64, (f64, f64))> = self.pts.iter().map(|&p| {
            let (plat, plon) = self.eval_normalized(p.0, p.1);
            ((plat - lat).powi(2) + (plon - lon).powi(2), p)
        }).collect();
        anchors.sort_by(|x, y| x.0.total_cmp(&y.0));
        starts.extend(anchors.into_iter().map(|(_, p)| p));

        starts.into_iter().find_map(|s| self.newton(s, lat, lon))
            .map(|(u, v)| ((u * self.scale + self.center.0) as f32, (v * self.scale + self.center.1) as f32))
    }

    /// Samples the forward map over `area` and bins the samples, so `inverse_transform` is
    /// restricted to (and fast on) the game's own domain.
    fn index_domain(&mut self, area: FoldCheckArea) {
        if !self.valid { return; }
        let (x0, y0, x1, y1, step) = area;
        let (nx, ny) = (((x1 - x0) / step) as usize + 1, ((y1 - y0) / step) as usize + 1);
        let mut samples = Vec::with_capacity(nx * ny);
        for j in 0..ny { for i in 0..nx {
            let u = ((x0 + i as f32 * step) as f64 - self.center.0) / self.scale;
            let v = ((y0 + j as f32 * step) as f64 - self.center.1) / self.scale;
            let (lat, lon) = self.eval_normalized(u, v);
            samples.push((lat, lon, u, v));
        }}
        // Bin size: largest gap between neighbouring samples, so any covered point has a sample
        // in its own or a neighbouring bin.
        let mut bin: f64 = 1e-9;
        for j in 0..ny { for i in 0..nx {
            let a = samples[j * nx + i];
            for (di, dj) in [(1, 0), (0, 1)] {
                if i + di < nx && j + dj < ny {
                    let b = samples[(j + dj) * nx + i + di];
                    bin = bin.max((a.0 - b.0).abs()).max((a.1 - b.1).abs());
                }
            }
        }}
        let mut index: std::collections::HashMap<(i64, i64), Vec<(f64, f64, f64, f64)>> = std::collections::HashMap::new();
        for s in samples {
            index.entry(((s.0 / bin).floor() as i64, (s.1 / bin).floor() as i64)).or_default().push(s);
        }
        self.domain = Some((bin, index));
    }

    /// Damped Newton solve of forward(u, v) = (lat, lon) in normalized coordinates.
    fn newton(&self, (mut u, mut v): (f64, f64), lat: f64, lon: f64) -> Option<(f64, f64)> {
        const H: f64 = 1e-5;
        let resid = |u: f64, v: f64| { let f = self.eval_normalized(u, v); (f.0 - lat, f.1 - lon) };
        let (mut r0, mut r1) = resid(u, v);
        for _ in 0..60 {
            if r0.abs() < 1e-8 && r1.abs() < 1e-8 { return Some((u, v)); }
            let (f0, f1) = (r0 + lat, r1 + lon);
            let (fu0, fu1) = self.eval_normalized(u + H, v);
            let (fv0, fv1) = self.eval_normalized(u, v + H);
            let (j00, j10, j01, j11) = ((fu0 - f0) / H, (fu1 - f1) / H, (fv0 - f0) / H, (fv1 - f1) / H);
            let d = j00 * j11 - j01 * j10;
            if d.abs() < 1e-15 { return None; }
            let (du, dv) = ((j11 * r0 - j01 * r1) / d, (j00 * r1 - j10 * r0) / d);
            // Backtrack until the residual shrinks.
            let norm = r0 * r0 + r1 * r1;
            let mut t = 1.0;
            loop {
                let (nu, nv) = (u - t * du, v - t * dv);
                let (n0, n1) = resid(nu, nv);
                if n0 * n0 + n1 * n1 < norm || t < 1e-4 {
                    (u, v, r0, r1) = (nu, nv, n0, n1);
                    break;
                }
                t *= 0.5;
            }
        }
        None
    }
}

/// Control points at the corners of `area`, placed by the anchors' least-squares affine fit (their
/// overall trend), so the warp interpolates over the whole map instead of extrapolating: a TPS's bending
/// grows without bound away from its anchors (r^2 ln r), while the map beyond them should follow the
/// anchors' logic. Corners that an anchor already occupies are skipped; with three anchors the fit is
/// exact and the corners change nothing.
fn trend_corners(cps: &[ControlPoint], area: FoldCheckArea) -> Vec<ControlPoint> {
    let n = cps.len() as f64;
    let (mx, my) = (cps.iter().map(|c| c.game_x as f64).sum::<f64>() / n, cps.iter().map(|c| c.game_y as f64).sum::<f64>() / n);
    let mut m = vec![vec![0.0; 3]; 3];
    let mut b = vec![[0.0; 2]; 3];
    for c in cps {
        let p = [1.0, c.game_x as f64 - mx, c.game_y as f64 - my];
        for i in 0..3 {
            for j in 0..3 { m[i][j] += p[i] * p[j]; }
            b[i][0] += p[i] * c.real_lat as f64;
            b[i][1] += p[i] * c.real_lon as f64;
        }
    }
    let Some(fit) = solve(m, b) else { return vec![] }; // collinear anchors
    let (x0, y0, x1, y1, _) = area;
    [("NW", x0, y0), ("NE", x1, y0), ("SW", x0, y1), ("SE", x1, y1)].into_iter()
        .filter(|&(_, x, y)| !cps.iter().any(|c| c.game_x == x && c.game_y == y))
        .map(|(name, x, y)| {
            let (dx, dy) = (x as f64 - mx, y as f64 - my);
            let at = |k: usize| fit[0][k] + fit[1][k] * dx + fit[2][k] * dy;
            ControlPoint { name: format!("{} corner (anchors' trend)", name), game_x: x, game_y: y, real_lat: at(0) as f32, real_lon: at(1) as f32 }
        })
        .collect()
}

const MILES_PER_DEGREE: f64 = 69.0;
/// The loosest warp tried (see `RubberSheet::smoothed`), and how many halvings below it are tried first,
/// tightest first.
const LOOSEST: f64 = 10.0;
const LOOSENING_STEPS: u32 = 20;
// ponytail: fixed clearance (fraction of |AB|) past the line; raise if folds remain near moved anchors.
const CROSSING_MARGIN: f64 = 0.02;
/// Fractions of the way across the line that are tried for each candidate move, smallest first.
const MOVE_FRACTIONS: [f64; 4] = [0.125, 0.25, 0.5, 1.0];

/// Signed doubled area of triangle (a, b, c); > 0 and < 0 are opposite windings.
fn orient(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

/// Game-space rectangle the warp must be fold-free over: (min_x, min_y, max_x, max_y, sample step).
pub type FoldCheckArea = (f32, f32, f32, f32, f32);

impl RubberSheet {
    /// Game positions inside `area` where the warp folds: where a sample-grid cell (the triangle to
    /// the next samples east and south) comes out turned over, so a crease narrower than the grid
    /// is caught too.
    pub fn folds(&self, area: FoldCheckArea) -> Vec<(f64, f64)> {
        let (x0, y0, x1, y1, step) = area;
        let h = step as f64;
        let mut out = Vec::new();
        let mut y = y0 as f64;
        while y <= y1 as f64 {
            let mut x = x0 as f64;
            while x <= x1 as f64 {
                let u = (x - self.center.0) / self.scale;
                let v = (y - self.center.1) / self.scale;
                let hn = h / self.scale;
                let p = self.eval_normalized(u, v);
                let px = self.eval_normalized(u + hn, v);
                let py = self.eval_normalized(u, v + hn);
                // Game (x east, y south) vs real (lat north, lon east): same handedness
                // means d(lat,lon)/d(x,y) has positive determinant.
                let det = (px.0 - p.0) * (py.1 - p.1) - (px.1 - p.1) * (py.0 - p.0);
                if det <= 0.0 { out.push((x, y)); }
                x += step as f64;
            }
            y += step as f64;
        }
        out
    }

    /// Builds a warp that is fold-free over `area`.
    ///
    /// An exact warp must fold where the game and the real world wind an anchor triangle
    /// differently. Only conflicts that actually produce a fold are resolved: among flipped
    /// triangles containing a fold, one corner is moved toward the line through the other two
    /// (part of the way, or just across it onto the game's side; its position along the line stays
    /// real-world). The smallest move that removes some folds is applied (if none does, the
    /// smallest full crossing), the warp rebuilt, and this repeats until no fold remains.
    /// Returns the warp and one report line per moved anchor (plus one if folds remain).
    pub fn new_fold_free(mut cps: Vec<ControlPoint>, area: FoldCheckArea) -> (Self, Vec<String>) {
        if cps.len() >= 3 { cps.extend(trend_corners(&cps, area)); }
        let n = cps.len();
        let mut sheet = RubberSheet::new(cps.clone());
        if n < 3 { return (sheet, vec![]); }
        // Local planar frame in miles, oriented like the game: x east, y south.
        let lat0 = cps.iter().map(|c| c.real_lat as f64).sum::<f64>() / n as f64;
        let kx = MILES_PER_DEGREE * lat0.to_radians().cos();
        let game: Vec<(f64, f64)> = cps.iter().map(|c| (c.game_x as f64, c.game_y as f64)).collect();
        let original: Vec<(f64, f64)> = cps.iter().map(|c| (c.real_lon as f64 * kx, -c.real_lat as f64 * MILES_PER_DEGREE)).collect();
        let mut real = original.clone();

        let mut folds = sheet.folds(area);
        let with = |cps: &[ControlPoint], r: usize, p: (f64, f64)| {
            let mut moved = cps.to_vec();
            moved[r].real_lon = (p.0 / kx) as f32;
            moved[r].real_lat = (-p.1 / MILES_PER_DEGREE) as f32;
            moved
        };
        for _ in 0..(4 * n) {
            if folds.is_empty() { break; }
            // Candidate moves: a corner of a flipped triangle containing a fold, part or all of the way
            // across the line through the other two (a fold is often cured well before the crossing).
            let mut candidates: Vec<(f64, usize, (f64, f64), f64)> = Vec::new();
            for a in 0..n { for b in a + 1..n { for c in b + 1..n {
                let og = orient(game[a], game[b], game[c]);
                if og * orient(real[a], real[b], real[c]) >= 0.0 { continue; }
                if !folds.iter().any(|&f| in_triangle(f, game[a], game[b], game[c])) { continue; }
                for (p, q, r) in [(a, b, c), (b, c, a), (c, a, b)] {
                    let full = across(real[p], real[q], real[r], og.signum());
                    for f in MOVE_FRACTIONS {
                        let to = (real[r].0 + (full.0 - real[r].0) * f, real[r].1 + (full.1 - real[r].1) * f);
                        candidates.push((((to.0 - real[r].0).powi(2) + (to.1 - real[r].1).powi(2)).sqrt(), r, to, f));
                    }
                }
            }}}
            if candidates.is_empty() { break } // folds not explained by a flipped triangle
            candidates.sort_by(|x, y| x.0.total_cmp(&y.0));
            // The smallest move that removes some folds; if none does, the smallest full crossing.
            let accepted = candidates.iter().find_map(|&(_, r, to, _)| {
                let trial = RubberSheet::new(with(&cps, r, to));
                let left = trial.folds(area);
                (left.len() < folds.len()).then_some((r, to, trial, left))
            });
            let (r, to, next, left) = accepted.unwrap_or_else(|| {
                let &(_, r, to, _) = candidates.iter().find(|c| c.3 == 1.0).unwrap();
                let trial = RubberSheet::new(with(&cps, r, to));
                let left = trial.folds(area);
                (r, to, trial, left)
            });
            real[r] = to;
            cps = with(&cps, r, to);
            sheet = next;
            folds = left;
        }

        // A game whose map disagrees with the world beyond what moving anchors across lines fixes (Fallout
        // 76's Appalachia rearranges West Virginia): the warp is loosened, as little as removes the folds, so
        // conflicting anchors give a little (each drifts from its real place by what is reported).
        let mut loosened = None;
        if !folds.is_empty() {
            for step in 0..LOOSENING_STEPS {
                let smoothing = LOOSEST * 0.5f64.powi((LOOSENING_STEPS - 1 - step) as i32);
                let trial = RubberSheet::smoothed(cps.clone(), smoothing);
                let left = trial.folds(area);
                if left.is_empty() || step == LOOSENING_STEPS - 1 {
                    (sheet, folds, loosened) = (trial, left, Some(smoothing));
                    break;
                }
            }
        }

        let mut report = Vec::new();
        if let Some(smoothing) = loosened {
            let drift = |c: &ControlPoint| {
                let (lat, lon) = sheet.transform(c.game_x, c.game_y);
                ((lat - c.real_lat) as f64 * MILES_PER_DEGREE).hypot((lon - c.real_lon) as f64 * kx)
            };
            let worst = cps.iter().map(|c| (drift(c), c.name.as_str())).max_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((miles, name)) = worst {
                report.push(format!("Warp loosened (smoothing {:.4}) to remove folds: anchors drift up to {:.1} mi ('{}')", smoothing, miles, name));
            }
        }
        for (i, cp) in cps.iter().enumerate() {
            let (de, ds) = (real[i].0 - original[i].0, real[i].1 - original[i].1);
            if de.abs() < 1e-6 && ds.abs() < 1e-6 { continue; }
            report.push(format!("Anchor '{}' moved {:.1} mi ({:+.1} mi N, {:+.1} mi E) to match the game's ordering",
                cp.name, (de * de + ds * ds).sqrt(), -ds, de));
        }
        if !folds.is_empty() {
            report.push(format!("Warp still folds at {} sample points (first at game {:?})", folds.len(), folds[0]));
        }
        sheet.index_domain(area);
        (sheet, report)
    }
}

fn in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let (d1, d2, d3) = (orient(a, b, p), orient(b, c, p), orient(c, a, p));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// Point c moved perpendicular to line a-b to just past it on side `side` (sign of orient(a, b, c)),
/// keeping its position along a-b.
fn across(a: (f64, f64), b: (f64, f64), c: (f64, f64), side: f64) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = ((c.0 - a.0) * dx + (c.1 - a.1) * dy) / (dx * dx + dy * dy);
    let s = side * CROSSING_MARGIN;
    (a.0 + t * dx - s * dy, a.1 + t * dy + s * dx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp(x: f32, y: f32, lat: f32, lon: f32) -> ControlPoint {
        ControlPoint { name: format!("{},{}", x, y), game_x: x, game_y: y, real_lat: lat, real_lon: lon }
    }

    #[test]
    fn map_corners_follow_the_anchors_trend() {
        // Anchors on an affine grid (0.01 deg per unit) except the centre, pulled off it.
        let cps = vec![
            cp(40.0, 40.0, 40.0, -80.0), cp(60.0, 40.0, 40.0, -79.8), cp(40.0, 60.0, 39.8, -80.0),
            cp(60.0, 60.0, 39.8, -79.8), cp(50.0, 50.0, 39.85, -79.85),
        ];
        let area = (0.0, 0.0, 100.0, 100.0, 5.0);
        let corners = trend_corners(&cps, area);
        assert_eq!(corners.len(), 4);
        let (sheet, report) = RubberSheet::new_fold_free(cps.clone(), area);
        assert!(report.is_empty(), "{:?}", report);
        // The anchors stay exact and the map's corners sit on the anchors' trend, not wherever the
        // spline's bending would fling them.
        for c in cps.iter().chain(&corners) {
            let (lat, lon) = sheet.transform(c.game_x, c.game_y);
            assert!((lat - c.real_lat).abs() < 1e-4 && (lon - c.real_lon).abs() < 1e-4, "{}: ({}, {})", c.name, lat, lon);
        }
        let nw = &corners[0];
        assert!((nw.real_lat - 40.39).abs() < 0.02 && (nw.real_lon + 80.39).abs() < 0.02, "{:?}", (nw.real_lat, nw.real_lon));
    }
}
