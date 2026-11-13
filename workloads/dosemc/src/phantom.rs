//! A voxel phantom: z-slabs along the beam axis, uniform in x/y within a lateral extent.
//! Each slab has a density (g/cm³); composition is water-equivalent everywhere (v1).

#[derive(Clone)]
pub struct Slab {
    pub name: &'static str,
    pub thickness_cm: f64,
    pub density: f64,
}

pub struct Phantom {
    pub slabs: Vec<Slab>,
    pub half_width_cm: f64, // lateral half-extent (x and y)
    pub dz_cm: f64,         // scoring voxel thickness along z
    pub z_edges: Vec<f64>,  // slab boundaries, starting at 0
}

impl Phantom {
    pub fn new(slabs: Vec<Slab>, half_width_cm: f64, dz_cm: f64) -> Self {
        let mut z_edges = vec![0.0];
        for s in &slabs {
            let last = *z_edges.last().unwrap();
            z_edges.push(last + s.thickness_cm);
        }
        Phantom { slabs, half_width_cm, dz_cm, z_edges }
    }
    pub fn depth_cm(&self) -> f64 {
        *self.z_edges.last().unwrap()
    }
    pub fn n_voxels(&self) -> usize {
        (self.depth_cm() / self.dz_cm).ceil() as usize
    }
    #[inline]
    pub fn density_at(&self, z: f64) -> f64 {
        for (i, s) in self.slabs.iter().enumerate() {
            if z < self.z_edges[i + 1] {
                return s.density;
            }
        }
        self.slabs.last().map(|s| s.density).unwrap_or(0.0)
    }
    /// Distance along +/-z to the next slab boundary from z moving with direction w (cos θ).
    #[inline]
    pub fn dist_to_boundary(&self, z: f64, w: f64) -> f64 {
        if w.abs() < 1e-12 {
            return f64::INFINITY;
        }
        for i in 0..self.slabs.len() {
            if z >= self.z_edges[i] && z < self.z_edges[i + 1] {
                let target = if w > 0.0 { self.z_edges[i + 1] } else { self.z_edges[i] };
                let d = (target - z) / w;
                return if d <= 0.0 { 1e-9 } else { d };
            }
        }
        f64::INFINITY
    }
    #[inline]
    pub fn inside(&self, x: f64, y: f64, z: f64) -> bool {
        z >= 0.0 && z < self.depth_cm() && x.abs() <= self.half_width_cm && y.abs() <= self.half_width_cm
    }
    pub fn water() -> Self {
        Phantom::new(vec![Slab { name: "water", thickness_cm: 30.0, density: 1.0 }], 15.0, 0.25)
    }
    /// The classic lung-interface benchmark: water / lung / water.
    pub fn lung_slab() -> Self {
        Phantom::new(
            vec![
                Slab { name: "water", thickness_cm: 3.0, density: 1.0 },
                Slab { name: "lung", thickness_cm: 8.0, density: 0.26 },
                Slab { name: "water", thickness_cm: 19.0, density: 1.0 },
            ],
            15.0,
            0.25,
        )
    }
}
