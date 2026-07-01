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
        Stream { name: "residue".into(),  sale_price: 50.0, quality: vec![ 0.0,  0.0,  0.0, 2.50] }, // fuel oil (discount to crude)
        Stream { name: "fcc_gaso".into(), sale_price: 0.0,  quality: vec![92.0,  8.0,  0.0, 0.10] },
        Stream { name: "lco".into(),      sale_price: 0.0,  quality: vec![ 0.0,  0.0, 45.0, 0.30] },
        Stream { name: "hc_distillate".into(), sale_price: 0.0, quality: vec![0.0, 0.0, 55.0, 0.05] }, // hydrocracker diesel, high cetane / low sulfur
        Stream { name: "lpg".into(),      sale_price: 60.0, quality: vec![ 0.0,  0.0,  0.0, 0.00] },
        Stream { name: "coke".into(),     sale_price: 0.0,  quality: vec![ 0.0,  0.0,  0.0, 0.00] }, // burned/slop
    ];
    let idx = |n: &str| streams.iter().position(|s| s.name == n).unwrap();

    let adu = Adu {
        name: "ADU".into(),
        capacity: 100_000.0,
        opex: 1.5,
    };

    // Two grades: light/sweet costs more but yields more valuable light cuts; heavy/sour
    // is cheaper but makes far more low-value residue. The LP blends them; which is best
    // shifts with the crack spread and the heavy-light differential.
    let crudes = vec![
        Crude {
            name: "Brent Light".into(),
            price: 66.0,        // benchmark + differential
            differential: 1.0,  // light/sweet premium; the solid default grade
            yields: vec![
                (idx("naphtha"), 0.25),
                (idx("gasoil"), 0.46), // distillate-rich
                (idx("residue"), 0.27),
            ],
        },
        Crude {
            name: "Urals Heavy".into(),
            price: 60.0,
            differential: -5.0, // heavy/sour discount: cheaper, but distillate-poor
            yields: vec![
                (idx("naphtha"), 0.17),
                (idx("gasoil"), 0.40),
                (idx("residue"), 0.42), // residue-heavy
            ],
        },
    ];

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

    // Hydrocracker: dormant until a capital project builds it (capacity 0). It upgrades
    // low-value residue into high-cetane, low-sulfur diesel (with volume gain from H2),
    // which is what makes cheap residue-heavy crude worth running — the "£80M bet on the
    // heavy-light spread" from the design doc.
    let hydrocracker = ConvUnit {
        name: "Hydrocracker".into(),
        feed_stream: idx("residue"),
        capacity: 0.0, // built via the "Build Hydrocracker" capital project
        modes: vec![Mode {
            name: "base".into(),
            severity: 0.70,
            opex: 15.0, // hydrogen + high pressure is genuinely opex-heavy
            yields: vec![
                (idx("hc_distillate"), 0.80),
                (idx("naphtha"), 0.15),
                (idx("lpg"), 0.08),
            ],
        }],
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
            allowed: vec![idx("gasoil"), idx("lco"), idx("hc_distillate")],
            specs: vec![
                Spec { property: 2, kind: SpecKind::Min, limit: 48.0 }, // cetane
                Spec { property: 3, kind: SpecKind::Max, limit: 0.5 },  // sulfur
            ],
        },
    ];

    Refinery {
        properties,
        streams,
        crudes,
        adu,
        conversions: vec![fcc, hydrocracker],
        products,
    }
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
    fn finances_reconcile_with_margin() {
        // The P&L breakdown must sum exactly to the reported margin — the whole point
        // of Phase A is that no number shown to the player is approximated.
        let r = phase0_refinery();
        let res = solve::solve(&r);
        let f = &res.finances;
        assert!((f.margin() - res.margin).abs() < 1e-6);
        assert!(
            (f.revenue() - f.crude_cost - f.opex - res.margin).abs() < 1e-6,
            "revenue − crude − opex must equal margin"
        );
        assert!(f.product_revenue > 0.0 && f.crude_cost > 0.0 && f.opex > 0.0);
    }

    #[test]
    fn tilt_steers_slate_but_not_reported_margin() {
        // A product-tilt preference may change the slate, but reported margin must stay
        // true economics (real prices), never inflated by the preference bonus.
        let r = phase0_refinery();
        let mut opts = solve::SolveOptions::default();
        opts.product_bonus = r
            .products
            .iter()
            .map(|p| if p.name == "diesel" { 8.0 } else { 0.0 })
            .collect();
        let res = solve::solve_opts(&r, &opts);
        // Margin still reconciles to the real-price breakdown (bonus excluded).
        assert!((res.finances.margin() - res.margin).abs() < 1e-6);
    }

    #[test]
    fn scaling_preserves_reconciliation_and_is_linear() {
        // Execution noise scales the whole solve; the P&L must still reconcile and the
        // result must shrink linearly (margin can't be created by under-running).
        let r = phase0_refinery();
        let full = solve::solve(&r);
        let f = 0.9;
        let scaled = full.scaled(f);
        assert!((scaled.finances.margin() - scaled.margin).abs() < 1e-6);
        assert!((scaled.margin - full.margin * f).abs() < 1e-3);
        assert!((scaled.crude_charge - full.crude_charge * f).abs() < 1e-3);
        assert!((scaled.finances.product_revenue - full.finances.product_revenue * f).abs() < 1e-3);
    }

    #[test]
    fn capacity_shadow_prices_nonnegative() {
        let r = phase0_refinery();
        for (name, sp) in solve::capacity_shadow_prices(&r, 100.0) {
            assert!(sp >= -1e-6, "shadow price for {name} negative: {sp}");
        }
    }

    #[test]
    fn serde_round_trip() {
        let r = phase0_refinery();
        let json = serde_json::to_string_pretty(&r).expect("serialize");
        let r2: model::Refinery = serde_json::from_str(&json).expect("deserialize");
        // Solve both and compare margins — proves the round-tripped model is identical.
        let res1 = solve::solve(&r);
        let res2 = solve::solve(&r2);
        assert!(
            (res1.margin - res2.margin).abs() < 1e-6,
            "margin mismatch after round-trip: {} vs {}",
            res1.margin,
            res2.margin
        );
    }
}
