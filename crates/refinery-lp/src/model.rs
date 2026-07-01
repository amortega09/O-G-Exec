//! Data model for the refinery. Deliberately a plain data description (no solver
//! types) so it can become serde/JSON in Phase 1 with zero structural change — this
//! is the "balancing is JSON edits, not code" surface from the design doc.

use serde::{Deserialize, Serialize};

/// A stream is any cut or unit product. It carries a raw-disposition price (fuel/LPG
/// sales, or 0 for slop/coke) and a quality vector — one blend-index value per
/// property in [`Refinery::properties`], same order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stream {
    pub name: String,
    /// £/bbl if sold/disposed as-is. 0.0 = free disposal sink (keeps byproducts from
    /// making the LP infeasible; the optimiser only dumps what it can't place).
    pub sale_price: f64,
    /// Blend-index value per property, aligned to `Refinery::properties`.
    pub quality: Vec<f64>,
}

/// A crude grade (assay): its cut yields and its market price. The ADU can charge a
/// blend across all available grades; the LP picks the optimal mix. Light/sweet grades
/// yield more valuable light cuts but cost more; heavy grades are cheaper but make more
/// low-value residue — the core crude-selection tradeoff.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Crude {
    pub name: String,
    /// £/bbl replacement cost (set by the market each tick = benchmark + `differential`).
    pub price: f64,
    /// Typical price offset to the crude benchmark (£/bbl): +ve premium (light/sweet),
    /// −ve discount (heavy/sour). Static grade characteristic; the sim adds the benchmark.
    pub differential: f64,
    /// (stream index, volumetric yield) per bbl of this crude charged.
    pub yields: Vec<(usize, f64)>,
}

/// Atmospheric distillation: the charge unit. Capacity + opex only; the assay (yields)
/// now lives on each [`Crude`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Adu {
    pub name: String,
    pub capacity: f64, // bbl/day
    pub opex: f64,     // £/bbl charged
}

/// One fixed linear operating recipe of a conversion unit. Severity is the *identity*
/// of the mode, not a coefficient multiplier — that is what keeps the model an LP
/// (formulation §3).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mode {
    pub name: String,
    pub severity: f64, // realised severity of this recipe, for the feed-weighted average
    pub opex: f64,     // £/bbl feed (higher-severity modes cost more)
    /// (stream index, volumetric yield) per bbl of feed.
    pub yields: Vec<(usize, f64)>,
}

/// A conversion unit (FCC, hydrocracker, …): consumes one feed stream, runs in one or
/// more parallel modes the LP blends feed across.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConvUnit {
    pub name: String,
    pub feed_stream: usize,
    pub capacity: f64, // bbl/day of feed
    pub modes: Vec<Mode>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SpecKind {
    Min,
    Max,
}

/// A linear product quality spec on one property's blend index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spec {
    pub property: usize,
    pub kind: SpecKind,
    pub limit: f64,
}

/// A finished product blend pool.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub name: String,
    pub price: f64,    // £/bbl
    pub demand: f64,   // bbl/day market ceiling
    pub contract: f64, // bbl/day floor the player committed to (0 = none)
    /// Stream indices allowed into this pool.
    pub allowed: Vec<usize>,
    pub specs: Vec<Spec>,
}

/// The whole single-period refinery configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Refinery {
    pub properties: Vec<String>,
    pub streams: Vec<Stream>,
    pub crudes: Vec<Crude>,
    pub adu: Adu,
    pub conversions: Vec<ConvUnit>,
    pub products: Vec<Product>,
}

impl Refinery {
    pub fn stream_idx(&self, name: &str) -> usize {
        self.streams
            .iter()
            .position(|s| s.name == name)
            .unwrap_or_else(|| panic!("unknown stream {name}"))
    }
}
