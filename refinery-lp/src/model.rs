//! Data model for the refinery. Deliberately a plain data description (no solver
//! types) so it can become serde/JSON in Phase 1 with zero structural change — this
//! is the "balancing is JSON edits, not code" surface from the design doc.

/// A stream is any cut or unit product. It carries a raw-disposition price (fuel/LPG
/// sales, or 0 for slop/coke) and a quality vector — one blend-index value per
/// property in [`Refinery::properties`], same order.
#[derive(Clone, Debug)]
pub struct Stream {
    pub name: String,
    /// £/bbl if sold/disposed as-is. 0.0 = free disposal sink (keeps byproducts from
    /// making the LP infeasible; the optimiser only dumps what it can't place).
    pub sale_price: f64,
    /// Blend-index value per property, aligned to `Refinery::properties`.
    pub quality: Vec<f64>,
}

/// Atmospheric distillation: the single charge unit. Linear assay split.
#[derive(Clone, Debug)]
pub struct Adu {
    pub name: String,
    pub capacity: f64,    // bbl/day
    pub opex: f64,        // £/bbl charged
    pub crude_price: f64, // £/bbl REPLACEMENT cost (see formulation §6 note)
    /// (stream index, volumetric yield) per bbl of crude charged.
    pub yields: Vec<(usize, f64)>,
}

/// One fixed linear operating recipe of a conversion unit. Severity is the *identity*
/// of the mode, not a coefficient multiplier — that is what keeps the model an LP
/// (formulation §3).
#[derive(Clone, Debug)]
pub struct Mode {
    pub name: String,
    pub severity: f64, // realised severity of this recipe, for the feed-weighted average
    pub opex: f64,     // £/bbl feed (higher-severity modes cost more)
    /// (stream index, volumetric yield) per bbl of feed.
    pub yields: Vec<(usize, f64)>,
}

/// A conversion unit (FCC, hydrocracker, …): consumes one feed stream, runs in one or
/// more parallel modes the LP blends feed across.
#[derive(Clone, Debug)]
pub struct ConvUnit {
    pub name: String,
    pub feed_stream: usize,
    pub capacity: f64, // bbl/day of feed
    pub modes: Vec<Mode>,
}

#[derive(Clone, Copy, Debug)]
pub enum SpecKind {
    Min,
    Max,
}

/// A linear product quality spec on one property's blend index.
#[derive(Clone, Debug)]
pub struct Spec {
    pub property: usize,
    pub kind: SpecKind,
    pub limit: f64,
}

/// A finished product blend pool.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct Refinery {
    pub properties: Vec<String>,
    pub streams: Vec<Stream>,
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
