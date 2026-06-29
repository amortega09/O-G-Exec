//! Phase 0 refinery LP spike. De-risks the single-period refinery LP (formulation
//! §9): one crude, ADU + FCC, two products, real solve, capacity shadow prices.

pub mod model;
pub mod solve;

use model::*;

/// The §9 minimal instance, hand-tuned so that *both* products have binding quality
/// specs and straight-run gasoil is genuinely contested between the diesel pool and FCC
/// feed — i.e. the toy already has the tensions the real game runs on.
pub fn phase0_refinery() -> Refinery {
    // properties: octane (min), rvp (max), cetane (min), sulfur (max)
    let properties = vec![
        "octane".into(),
        "rvp".into(),
        "cetane".into(),
        "sulfur".into(),
    ];

    // quality vectors aligned to `properties`.
    let streams = vec![
        Stream { name: "naphtha".into(),  sale_price: 70.0, quality: vec![70.0, 12.0,  0.0, 0.05] }, // petchem outlet
        Stream { name: "gasoil".into(),   sale_price: 0.0,  quality: vec![ 0.0,  0.0, 50.0, 0.60] }, // must be processed
        Stream { name: "residue".into(),  sale_price: 55.0, quality: vec![ 0.0,  0.0,  0.0, 2.50] }, // fuel oil
        Stream { name: "fcc_gaso".into(), sale_price: 0.0,  quality: vec![92.0,  8.0,  0.0, 0.10] },
        Stream { name: "lco".into(),      sale_price: 0.0,  quality: vec![ 0.0,  0.0, 45.0, 0.30] },
        Stream { name: "lpg".into(),      sale_price: 60.0, quality: vec![ 0.0,  0.0,  0.0, 0.00] },
        Stream { name: "coke".into(),     sale_price: 0.0,  quality: vec![ 0.0,  0.0,  0.0, 0.00] }, // burned/slop
    ];
    let idx = |n: &str| streams.iter().position(|s| s.name == n).unwrap();

    let adu = Adu {
        name: "ADU".into(),
        capacity: 100_000.0,
        opex: 1.5,
        crude_price: 65.0,
        yields: vec![
            (idx("naphtha"), 0.25),
            (idx("gasoil"), 0.45),
            (idx("residue"), 0.30),
        ],
    };

    let fcc = ConvUnit {
        name: "FCC".into(),
        feed_stream: idx("gasoil"),
        capacity: 50_000.0,
        modes: vec![
            Mode {
                name: "low_sev".into(),
                severity: 0.60,
                opex: 3.0,
                yields: vec![
                    (idx("fcc_gaso"), 0.50),
                    (idx("lco"), 0.30),
                    (idx("lpg"), 0.06),
                    (idx("coke"), 0.05),
                ],
            },
            Mode {
                name: "high_sev".into(),
                severity: 0.85,
                opex: 4.5,
                yields: vec![
                    (idx("fcc_gaso"), 0.58),
                    (idx("lco"), 0.18),
                    (idx("lpg"), 0.10),
                    (idx("coke"), 0.08),
                ],
            },
        ],
    };

    let products = vec![
        Product {
            name: "gasoline".into(),
            price: 95.0,
            demand: 60_000.0,
            contract: 0.0,
            allowed: vec![idx("naphtha"), idx("fcc_gaso")],
            specs: vec![
                Spec { property: 0, kind: SpecKind::Min, limit: 90.0 }, // octane
                Spec { property: 1, kind: SpecKind::Max, limit: 9.0 },  // rvp
            ],
        },
        Product {
            name: "diesel".into(),
            price: 90.0,
            demand: 60_000.0,
            contract: 0.0,
            allowed: vec![idx("gasoil"), idx("lco")],
            specs: vec![
                Spec { property: 2, kind: SpecKind::Min, limit: 48.0 }, // cetane
                Spec { property: 3, kind: SpecKind::Max, limit: 0.5 },  // sulfur
            ],
        },
    ];

    Refinery { properties, streams, adu, conversions: vec![fcc], products }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_with_sane_economics() {
        let r = phase0_refinery();
        let res = solve::solve(&r);

        // Positive margin, and we actually run crude.
        assert!(res.margin > 0.0, "margin should be positive, got {}", res.margin);
        assert!(res.crude_charge > 0.0);
        // Cannot exceed installed capacity.
        assert!(res.crude_charge <= r.adu.capacity + 1e-6);
        let fcc = &res.conversions[0];
        assert!(fcc.throughput <= fcc.capacity + 1e-6);

        // Gasoline octane spec must hold on the realised blend (>= 90).
        let g = res.products.iter().find(|p| p.name == "gasoline").unwrap();
        if g.volume > 1e-6 {
            let oct: f64 = g.blend.iter()
                .map(|(s, v)| r.streams[r.stream_idx(s)].quality[0] * v)
                .sum::<f64>() / g.volume;
            assert!(oct >= 90.0 - 1e-6, "octane {oct} below spec");
        }
        // Diesel sulfur spec must hold (<= 0.5).
        let d = res.products.iter().find(|p| p.name == "diesel").unwrap();
        if d.volume > 1e-6 {
            let sul: f64 = d.blend.iter()
                .map(|(s, v)| r.streams[r.stream_idx(s)].quality[3] * v)
                .sum::<f64>() / d.volume;
            assert!(sul <= 0.5 + 1e-6, "sulfur {sul} above spec");
        }
    }

    #[test]
    fn capacity_shadow_prices_nonnegative() {
        let r = phase0_refinery();
        for (name, sp) in solve::capacity_shadow_prices(&r, 100.0) {
            assert!(sp >= -1e-6, "shadow price for {name} negative: {sp}");
        }
    }
}
