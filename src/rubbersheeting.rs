// Smooth Rubbersheeting using Affine Transformation + Inverse Distance Weighting (IDW) of Residuals
// This approach provides a global trend (Affine) for extrapolation and a local warp (IDW) for correcting specific locations.
// Eliminates triangulation artifacts (creases, straight lines) and provides "averaged" movement as requested.

#[derive(Debug, Clone)]
pub struct ControlPoint {
    pub name: String,
    pub game_x: f32,
    pub game_y: f32,
    pub real_lat: f32,
    pub real_lon: f32,
}

pub struct RubberSheet {
    control_points: Vec<ControlPoint>,
    // Affine coefficients for GameX = A*Lat + B*Lon + C
    affine_x: (f32, f32, f32), 
    // Affine coefficients for GameY = D*Lat + E*Lon + F
    affine_y: (f32, f32, f32),
    // Residuals for Inverse IDW: (res_x, res_y)
    residuals_inv: Vec<(f32, f32)>,
    
    // FORWARD Affine coefficients for RealLat = A*GameX + B*GameY + C
    affine_lat: (f32, f32, f32),
    // FORWARD Affine coefficients for RealLon = D*GameX + E*GameY + F
    affine_lon: (f32, f32, f32),
    // Residuals for Forward IDW: (res_lat, res_lon)
    residuals_fwd: Vec<(f32, f32)>,
}

impl RubberSheet {
    pub fn new(control_points: Vec<ControlPoint>) -> Self {
        if control_points.len() < 3 {
             return RubberSheet {
                control_points,
                affine_x: (0.0, 0.0, 0.0),
                affine_y: (0.0, 0.0, 0.0),
                residuals_inv: Vec::new(),
                affine_lat: (0.0, 0.0, 0.0),
                affine_lon: (0.0, 0.0, 0.0),
                residuals_fwd: Vec::new(),
            };
        }

        // --- 1. Inverse Affine (Real -> Game) ---
        let (affine_x, affine_y, residuals_inv) = Self::calculate_affine(&control_points, true);
        
        // --- 2. Forward Affine (Game -> Real) ---
        let (affine_lat, affine_lon, residuals_fwd) = Self::calculate_affine(&control_points, false);
        
        RubberSheet { 
            control_points, 
            affine_x, affine_y, residuals_inv,
            affine_lat, affine_lon, residuals_fwd 
        }
    }
    
    fn calculate_affine(control_points: &[ControlPoint], inverse: bool) -> ((f32,f32,f32), (f32,f32,f32), Vec<(f32,f32)>) {
        let n = control_points.len() as f32;
        let mut sum_u = 0.0;
        let mut sum_v = 0.0;
        let mut sum_u2 = 0.0;
        let mut sum_v2 = 0.0;
        let mut sum_uv = 0.0;
        
        let mut sum_out1 = 0.0;
        let mut sum_out2 = 0.0;
        let mut sum_u_out1 = 0.0;
        let mut sum_v_out1 = 0.0;
        let mut sum_u_out2 = 0.0;
        let mut sum_v_out2 = 0.0;
        
        for cp in control_points {
            let (u, v, out1, out2) = if inverse {
                (cp.real_lat, cp.real_lon, cp.game_x, cp.game_y)
            } else {
                (cp.game_x, cp.game_y, cp.real_lat, cp.real_lon)
            };
            
            sum_u += u;
            sum_v += v;
            sum_u2 += u * u;
            sum_v2 += v * v;
            sum_uv += u * v;
            
            sum_out1 += out1;
            sum_out2 += out2;
            sum_u_out1 += u * out1;
            sum_v_out1 += v * out1;
            sum_u_out2 += u * out2;
            sum_v_out2 += v * out2;
        }
        
        let det = sum_u2 * (sum_v2 * n - sum_v * sum_v) -
                  sum_uv * (sum_uv * n - sum_v * sum_u) +
                  sum_u * (sum_uv * sum_v - sum_v2 * sum_u);
                  
        if det.abs() < 1e-6 {
             return ((0.0,0.0,0.0), (0.0,0.0,0.0), Vec::new());
        }

        let inv_det = 1.0 / det;
        
        let m00 = sum_v2 * n - sum_v * sum_v;
        let m01 = sum_u * sum_v - sum_uv * n;
        let m02 = sum_uv * sum_v - sum_u * sum_v2;
        
        let m10 = m01;
        let m11 = sum_u2 * n - sum_u * sum_u;
        let m12 = sum_u * sum_uv - sum_u2 * sum_v;
        
        let m20 = m02;
        let m21 = m12;
        let m22 = sum_u2 * sum_v2 - sum_uv * sum_uv;
        
        let a1 = (m00 * sum_u_out1 + m01 * sum_v_out1 + m02 * sum_out1) * inv_det;
        let b1 = (m10 * sum_u_out1 + m11 * sum_v_out1 + m12 * sum_out1) * inv_det;
        let c1 = (m20 * sum_u_out1 + m21 * sum_v_out1 + m22 * sum_out1) * inv_det;
        
        let a2 = (m00 * sum_u_out2 + m01 * sum_v_out2 + m02 * sum_out2) * inv_det;
        let b2 = (m10 * sum_u_out2 + m11 * sum_v_out2 + m12 * sum_out2) * inv_det;
        let c2 = (m20 * sum_u_out2 + m21 * sum_v_out2 + m22 * sum_out2) * inv_det;
        
        let mut residuals = Vec::new();
        for cp in control_points {
             let (u, v, actual1, actual2) = if inverse {
                (cp.real_lat, cp.real_lon, cp.game_x, cp.game_y)
            } else {
                (cp.game_x, cp.game_y, cp.real_lat, cp.real_lon)
            };
            
            let pred1 = a1 * u + b1 * v + c1;
            let pred2 = a2 * u + b2 * v + c2;
            residuals.push((actual1 - pred1, actual2 - pred2));
        }
        
        println!("[Rubbersheeting] Affine Loop (Inverse={}): Out1 = {:.2}*U + {:.2}*V + {:.2}", inverse, a1, b1, c1);
        
        ((a1, b1, c1), (a2, b2, c2), residuals)
    }
    
    // Transform Game -> Real (Forward)
    // Uses Forward Affine + IDW of Residuals
    pub fn transform(&self, game_x: f32, game_y: f32) -> (f32, f32) {
        if self.control_points.len() < 3 { return (0.0, 0.0); }
        
        // 1. Calculate Affine Base
        let (a, b, c) = self.affine_lat;
        let (d, e, f) = self.affine_lon;
        
        let base_lat = a * game_x + b * game_y + c;
        let base_lon = d * game_x + e * game_y + f;
        
        // 2. Interpolate Residuals using IDW
        let mut weight_sum = 0.0;
        let mut res_sum_lat = 0.0;
        let mut res_sum_lon = 0.0;
        let p = 2.0; 
        
        for (i, cp) in self.control_points.iter().enumerate() {
            let dist_sq = (game_x - cp.game_x).powi(2) + (game_y - cp.game_y).powi(2);
            
            if dist_sq < 1e-4 {
                let (rl, rlo) = self.residuals_fwd[i];
                return (base_lat + rl, base_lon + rlo);
            }
            
            let w = 1.0 / dist_sq.powf(p/2.0);
            weight_sum += w;
            
            let (rl, rlo) = self.residuals_fwd[i];
            res_sum_lat += w * rl;
            res_sum_lon += w * rlo;
        }
        
        if weight_sum > 0.0 {
            (base_lat + res_sum_lat / weight_sum, base_lon + res_sum_lon / weight_sum)
        } else {
            (base_lat, base_lon)
        }
    }

    /// Inverse Transform: Real World (Lat, Lon) -> Game Coordinates (X, Y)
    /// Algorithm: Affine Base + IDW Interpolation of Residuals
    pub fn inverse_transform(&self, lat: f32, lon: f32) -> Option<(f32, f32)> {
        if self.control_points.len() < 3 { return None; }
        
        // 1. Calculate Affine Base
        let (a, b, c) = self.affine_x;
        let (d, e, f) = self.affine_y;
        
        let base_x = a * lat + b * lon + c;
        let base_y = d * lat + e * lon + f;
        
        // 2. Interpolate Residuals using IDW
        let mut weight_sum = 0.0;
        let mut res_sum_x = 0.0;
        let mut res_sum_y = 0.0;
        let p = 2.0; // Standard Gravity Model Power
        
        for (i, cp) in self.control_points.iter().enumerate() {
            let dist_sq = (lat - cp.real_lat).powi(2) + (lon - cp.real_lon).powi(2);
            
            // Exact match check
            if dist_sq < 1e-8 {
                let (rx, ry) = self.residuals_inv[i];
                return Some((base_x + rx, base_y + ry));
            }
            
            let w = 1.0 / dist_sq.powf(p/2.0);
            weight_sum += w;
            
            let (rx, ry) = self.residuals_inv[i];
            res_sum_x += w * rx;
            res_sum_y += w * ry;
        }
        
        if weight_sum > 0.0 {
            let final_x = base_x + (res_sum_x / weight_sum);
            let final_y = base_y + (res_sum_y / weight_sum);
            Some((final_x, final_y))
        } else {
            // Should not happen unless points are infinite distance away
            Some((base_x, base_y))
        }
    }
}    

