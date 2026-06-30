//! Deterministic PRNG and per-subsystem stream splitting.
//!
//! One master seed derives several *independent* streams (market, reliability, …).
//! Independence matters: adding or reordering a subsystem must not shift another
//! subsystem's rolls, so balancing stays stable and replays stay reproducible. This is
//! the foundation the stochastic spine (hazards, execution noise, events) sits on.

use serde::{Deserialize, Serialize};

/// Simple xorshift64 PRNG — deterministic, no-std-friendly, tiny.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Approximate standard normal via Box-Muller.
    pub fn normal(&mut self) -> f64 {
        let u1 = self.next_f64().max(1e-15); // avoid log(0)
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }

    /// Returns true with probability `p` (clamped to [0,1]).
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p.clamp(0.0, 1.0)
    }
}

/// SplitMix64 finaliser — turns a counter into a well-distributed seed.
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Independent PRNG streams, one per stochastic subsystem. Each is seeded by mixing the
/// master seed with a distinct stream index, so the streams don't correlate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RngStreams {
    pub market: Rng,
    pub reliability: Rng,
    pub execution: Rng,
    pub events: Rng,
}

impl RngStreams {
    pub fn from_seed(master: u64) -> Self {
        let s = |i: u64| Rng::new(splitmix64(master.wrapping_add(i.wrapping_mul(0x9E3779B97F4A7C15))));
        Self {
            market: s(0),
            reliability: s(1),
            execution: s(2),
            events: s(3),
        }
    }
}
