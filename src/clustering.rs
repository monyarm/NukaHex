use crate::schema::FixedLocation;

/// Clusters locations based on a simple distance metric.
/// If any point in a cluster is within `distance_miles` of a point in another cluster (or an unclustered point),
/// they are grouped together.
pub fn cluster_locations<'a>(locations: &[&'a FixedLocation], distance_miles: f32) -> Vec<Vec<&'a FixedLocation>> {
    let mut clusters: Vec<Vec<&'a FixedLocation>> = Vec::new();

    for loc in locations {
        let mut added_to_cluster = false;
        // Check existing clusters
        for cluster in &mut clusters {
            // Check if ANY point in the cluster is close enough to ANY point in the new cluster 
            // (Transitive clustering: A near B, B near C -> A, B, C cluster)
            // Dist check in grid units (6 miles/hex at scale 6.0)
            // FixedData was generated at 6.0 roughly, but x/y are grid coordinates.
            // The distance passed in is in MILES.
            // We assume the grid coordinates (x, y) represent 6.0 miles per unit (from the typical fix resolution).
            // NOTE: This logic assumes the inputs are from the standard 6.0 mi/hex grid.
            // A more robust solution would be to use real_lat/real_lon if available, but x/y is what we have consistent.
            
            if cluster.iter().any(|existing| {
                let dx = loc.x - existing.x;
                let dy = loc.y - existing.y;
                let dist_hexes = (dx*dx + dy*dy).sqrt();
                let dist_miles_calc = dist_hexes * 6.0; 
                dist_miles_calc <= distance_miles 
            }) {
                cluster.push(loc);
                added_to_cluster = true;
                break; 
            }
        }
        if !added_to_cluster {
            clusters.push(vec![loc]);
        }
    }

    // Merge pass (Greedy initial clustering might miss bridges)
    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0;
        while i < clusters.len() {
            let mut j = i + 1;
            while j < clusters.len() {
                let should_merge = clusters[i].iter().any(|a| {
                    clusters[j].iter().any(|b| {
                        let dx = a.x - b.x;
                        let dy = a.y - b.y;
                        let dist = (dx*dx + dy*dy).sqrt() * 6.0;
                        dist <= distance_miles
                    })
                });

                if should_merge {
                    let mut other = clusters.remove(j);
                    clusters[i].append(&mut other);
                    merged = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    
    clusters
}
