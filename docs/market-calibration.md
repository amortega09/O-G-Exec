# Market Calibration — anchored to real oil-market data

The market model is tuned to reproduce the *statistical behaviour* of real crude and
refining margins, not a specific price path. Targets and the historical data behind them
are below; the OU + shock parameters in `data/scenarios/*.json` are derived to hit them.

All figures are GBP (≈ USD ÷ 1.27, the rough long-run cable rate). The game's crude is a
Brent proxy.

## 1. Crude (Brent) — historical reference

Brent annual averages, USD/bbl (rounded), 2008–2025:

| 08 | 09 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 | 22 | 23 | 24 | 25 |
|----|----|----|----|----|----|----|----|----|----|----|----|----|----|----|----|----|----|
| 97 | 62 | 80 |111 |112 |109 | 99 | 52 | 44 | 54 | 71 | 64 | 42 | 71 |101 | 82 | 80 | 70 |

- **Intra-period range:** ~$19 (Apr 2020) to ~$147 (Jul 2008); ≈ **£15–£116**.
- **Long-run mean:** ≈ $76 ≈ **£60–65**.
- **Annualized volatility of returns:** ~**35%** in normal years (30–40%), spiking past
  70% in 2008/2020 crises.
- **Mean reversion is weak:** oil *trends for years* (2011–14 plateau ~$110; 2015–17
  trough ~$50; 2021–22 surge). Shock half-life is months-to-years, not weeks.

**Derived OU params (weekly):**
- `crude_mean` 65 · `crude_volatility` 2.9 (≈4.5%/wk → ~32% annualized base, shocks add
  the fat tail to ~37%) · `crude_reversion` 0.02 (half-life ≈ 35 wk ≈ 0.8 yr → long trends).
- Stationary sd ≈ σ/√(2θ) ≈ £14–15 around the mean *plus* shock excursions ⇒ a realistic
  ~£25–115 lived-in range.

## 2. Crack spreads (gross product premium over crude)

Real single-product gross cracks, USD/bbl:
- **Gasoline (RBOB–Brent):** normal **$8–20**, summer-peaked; 2022 spiked to ~$40–50.
- **Diesel/gasoil (ULSD/gasoil–Brent):** normal **$12–25**; 2022 diesel crisis ~$60–70.
- Cracks are **volatile and weakly mean-reverting**, widening sharply when supply is tight
  or refining capacity is short.
- **Seasonality:** gasoline cracks peak in summer (driving season); middle-distillate
  (diesel/heating) cracks peak in winter.

**Derived params:**
- `gasoline_spread_mean` 28 · `diesel_spread_mean` 24 (GBP premium incl. our simple-plant
  framing) · `spread_volatility` 2.5 · `spread_reversion` 0.06 (cracks trend, don't snap
  back) · `gasoline_seasonal_amplitude` 7 (summer) · `diesel_seasonal_amplitude` 6 (winter).

Resulting refinery gross margin lands ~£3–6/bbl normal — in band for a simple ADU+FCC
($2–8/bbl), with idle weeks when cracks compress (real plants cut runs in poor margins).

## 3. Shock events (the fat tail)

Real oil's defining feature is violent, discrete shocks. Modelled as low-probability
multiplicative jumps to the price level/cracks that then decay via the weak reversion —
exactly how a real supply shock spikes and fades. Frequencies/magnitudes are sized from
history:

| Shock | Models | ~Frequency | Effect |
|---|---|---|---|
| Supply disruption | 1990 Gulf, 2022 Russia, Libya '11 | ~1 / 5 yr | crude ×1.25–1.45 |
| Demand collapse | 2008 GFC, 2020 COVID | ~1 / 8 yr | crude ×0.55–0.75 |
| OPEC supply action | routine OPEC+ cuts/boosts | ~1 / yr | crude ×0.88–1.12 |
| Refining-margin spike | 2022 diesel crisis, hurricane outages | ~1 / 3 yr | cracks ×1.5–2.2 |

Over a 30-year campaign a player should live through a few majors — a boom that funds
expansion, a crash that tests the balance sheet — which is what makes timing, leverage,
and hedging decisions matter the way they do in the real industry.

## 4. Validation

The headless analysis (run on demand) should show, for the recalibrated market:
- crude annualized vol ≈ 33–40%, lived-in range ≈ £25–115;
- crack means/vols in the bands above, with seasonal structure;
- a few shocks per multi-decade run;
- passivity now *sometimes* fails (a crash can bankrupt an unmaintained, unhedged plant),
  while invest-and-maintain still wins — i.e. realism *and* a real decision.
