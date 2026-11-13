//! xoshiro256** — small, fast, fp64-friendly, no dependencies. One instance per thread,
//! seeded by SplitMix64 from (seed, thread id) so runs are reproducible per thread count.

pub struct Rng([u64; 4]);

impl Rng {
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut s = seed ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut next = || {
            s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = s;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Rng([next(), next(), next(), next()])
    }
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.0;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }
    /// Uniform in (0, 1]: never returns 0, so ln(u) is always finite.
    #[inline]
    pub fn uniform(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 1.0) * (1.0 / 9_007_199_254_740_992.0)
    }
}
