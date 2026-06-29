//! Phase 0 spike runner: solve the §9 instance, print the slate, capacity shadow
//! prices, and the solve time (proving it's trivial).

use std::time::Instant;

fn main() {
    let r = refinery_lp::phase0_refinery();

    let t = Instant::now();
    let res = refinery_lp::solve::solve(&r);
    let solve_us = t.elapsed().as_micros();

    println!("=== Phase 0 refinery LP ===");
    println!("daily contribution margin : £{:>12.0}", res.margin);
    println!("solve time                : {solve_us} µs\n");

    println!("ADU charge : {:>9.0} / {:>9.0} bbl/d", res.crude_charge, res.adu.capacity);
    for u in &res.conversions {
        println!(
            "{:<4} feed  : {:>9.0} / {:>9.0} bbl/d   severity {:.3}",
            u.name, u.throughput, u.capacity, u.realised_severity
        );
        for (m, v) in &u.per_mode {
            println!("       {:<9} {:>9.0} bbl/d", m, v);
        }
    }

    println!("\nproducts:");
    for p in &res.products {
        println!("  {:<8} {:>9.0} bbl/d", p.name, p.volume);
        for (s, v) in &p.blend {
            println!("     <- {:<9} {:>9.0}", s, v);
        }
    }

    if !res.sales.is_empty() {
        println!("\nraw sales:");
        for (s, v) in &res.sales {
            println!("  {:<8} {:>9.0} bbl/d", s, v);
        }
    }

    println!("\ncapacity shadow prices (£/day per bbl/d, +100 bbl/d step):");
    for (name, sp) in refinery_lp::solve::capacity_shadow_prices(&r, 100.0) {
        println!("  {name:<5} £{sp:>7.3}");
    }
}
