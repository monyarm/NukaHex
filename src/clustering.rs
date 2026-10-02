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
            // Check if ANY point in the cluster is close enough
            // (Transitive clustering: A near B, B near C -> A, B, C cluster)
            if cluster.iter().any(|existing| {
                dist_miles(loc, existing) <= distance_miles
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
                        dist_miles(a, b) <= distance_miles
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

/// True ground distance between two placed locations, from their lat/lon (independent of the
/// grid resolution the locations were placed on). Locations without lat/lon are never close.
fn dist_miles(a: &FixedLocation, b: &FixedLocation) -> f32 {
    match (a.real_lat, a.real_lon, b.real_lat, b.real_lon) {
        (Some(la), Some(oa), Some(lb), Some(ob)) => {
            let dy = (la - lb) * 69.0;
            let dx = (oa - ob) * 69.0 * ((la + lb) / 2.0).to_radians().cos();
            (dx * dx + dy * dy).sqrt()
        }
        _ => f32::INFINITY,
    }
}
