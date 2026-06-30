/**
 * app.js — Overhauled Game Controller for O&G Exec.
 *
 * Implements a high-fidelity tabbed sidebar interface, live animated SVG refinery
 * schematic with bottleneck highlighting, interactive circular telemetry dials,
 * credit debt control facility, and trend arrows for key stats.
 */

import { drawHistoryChart } from './charts.js';

// ── State ───────────────────────────────────────────────────────────────────
let game = null;           // WASM Game instance
let gameView = null;       // Latest GameView from WASM
let victoryTarget = 500_000_000;
let tickSpeed = 0;         // 0=paused, 1=1×, 5=5×
let tickTimer = null;
let pendingActions = [];
let lastEventCount = 0;

// Stats history for trend computation (Bloomberg style)
let prevStats = {
  cash: null,
  valuation: null,
  debt: null
};

// Tick intervals by speed setting (ms between ticks)
const TICK_INTERVALS = { 1: 600, 5: 120 };

// ── Initialisation ──────────────────────────────────────────────────────────
async function init() {
  try {
    // Load WASM module
    const wasm = await import('../pkg/wasm_bridge.js');
    await wasm.default();

    // Load scenario and refinery data
    const [scenarioRes, refineryRes] = await Promise.all([
      fetch('/data/scenarios/tutorial.json'),
      fetch('/data/refinery.json'),
    ]);

    const scenarioJson = await scenarioRes.text();
    const refineryJson = await refineryRes.text();

    const config = JSON.parse(scenarioJson);
    victoryTarget = config.victory_valuation || 500_000_000;

    // Create game instance
    const seed = BigInt(Date.now());
    game = new wasm.Game(scenarioJson, refineryJson, seed);

    // Get initial view
    gameView = game.view();

    // Setup events and inputs
    setupTabs();
    setupControls();
    setupSidebarBehavior();
    renderAll(gameView);

    // Fade out loading screen
    const loading = document.getElementById('loading-screen');
    loading.classList.add('fade-out');
    setTimeout(() => {
      loading.classList.add('hidden');
    }, 400);

    // Start in paused state
    setSpeed(0);

  } catch (err) {
    console.error('Failed to initialise O&G Exec:', err);
    document.querySelector('.loading-subtitle').textContent =
      `Error: ${err.message}. Ensure WASM is built.`;
  }
}

// ── Tabs Switching ──────────────────────────────────────────────────────────
function setupTabs() {
  const btns = document.querySelectorAll('.nav-btn');
  const panels = document.querySelectorAll('.tab-panel');

  btns.forEach(btn => {
    btn.addEventListener('click', () => {
      const tabId = btn.getAttribute('data-tab');
      
      // Update sidebar nav states
      btns.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');

      // Toggle panels
      panels.forEach(p => p.classList.add('hidden'));
      document.getElementById(`tab-${tabId}`).classList.remove('hidden');

      // Re-trigger layout-sensitive charts when showing finance tab
      if (tabId === 'finance' && gameView) {
        setTimeout(() => {
          renderChart(gameView);
        }, 50);
      }
    });
  });
}

// ── Game Loop ───────────────────────────────────────────────────────────────
function doTick() {
  if (!game || !game.is_running()) return;

  // Swap pending action list
  const actionsJson = JSON.stringify(pendingActions);
  pendingActions = [];

  try {
    // Record current stats to calculate trends on next render
    if (gameView) {
      prevStats.cash = gameView.cash;
      prevStats.valuation = gameView.valuation;
      prevStats.debt = gameView.debt;
    }

    gameView = game.tick(actionsJson);
    renderAll(gameView);

    // Reset debt slider to reflect new state
    updateDebtSliderState(gameView);

    // Check end condition
    if (gameView.status !== 'Running') {
      setSpeed(0);
      showGameOver(gameView.status);
    }
  } catch (err) {
    console.error('Tick error:', err);
    setSpeed(0);
  }
}

function setSpeed(speed) {
  tickSpeed = speed;
  if (tickTimer) {
    clearInterval(tickTimer);
    tickTimer = null;
  }

  // Update control button states
  document.getElementById('btn-pause').classList.toggle('active', speed === 0);
  document.getElementById('btn-play').classList.toggle('active', speed === 1);
  document.getElementById('btn-fast').classList.toggle('active', speed >= 5);

  if (speed > 0) {
    const interval = TICK_INTERVALS[speed] || 600;
    tickTimer = setInterval(doTick, interval);
  }
}

// ── Controls & Actions ──────────────────────────────────────────────────────
function setupControls() {
  // Speed buttons
  document.getElementById('btn-pause').addEventListener('click', () => setSpeed(0));
  document.getElementById('btn-play').addEventListener('click', () => setSpeed(1));
  document.getElementById('btn-fast').addEventListener('click', () => setSpeed(5));
  
  document.getElementById('btn-step').addEventListener('click', () => {
    setSpeed(0);
    doTick();
  });

  // Severity controls (+/- adjust)
  const sevSlider = document.getElementById('slider-severity');
  const sevVal = document.getElementById('severity-value');
  
  const updateSeverity = (val) => {
    val = Math.max(0.0, Math.min(1.0, val));
    sevSlider.value = Math.round(val * 100);
    sevVal.textContent = val.toFixed(2);
    pendingActions.push({ SetSeverity: val });
  };

  sevSlider.addEventListener('input', () => {
    updateSeverity(sevSlider.value / 100);
  });

  document.getElementById('btn-sev-down').addEventListener('click', () => {
    updateSeverity((parseFloat(sevVal.textContent) - 0.05));
  });

  document.getElementById('btn-sev-up').addEventListener('click', () => {
    updateSeverity((parseFloat(sevVal.textContent) + 0.05));
  });

  // Product Tilt controls
  const tiltSlider = document.getElementById('slider-tilt');
  const tiltVal = document.getElementById('tilt-value');

  const updateTilt = (val) => {
    val = Math.max(-1.0, Math.min(1.0, val));
    tiltSlider.value = Math.round(val * 100);
    
    if (Math.abs(val) < 0.05) {
      tiltVal.textContent = 'Neutral';
    } else if (val < 0) {
      tiltVal.textContent = `Gasoline (${Math.abs(val).toFixed(2)})`;
    } else {
      tiltVal.textContent = `Diesel (${val.toFixed(2)})`;
    }
    pendingActions.push({ SetProductTilt: val });
  };

  tiltSlider.addEventListener('input', () => {
    updateTilt(tiltSlider.value / 100);
  });

  document.getElementById('btn-tilt-down').addEventListener('click', () => {
    const current = tiltVal.textContent === 'Neutral' ? 0.0 : (tiltVal.textContent.startsWith('Gasoline') ? -parseFloat(tiltSlider.value / 100) : parseFloat(tiltSlider.value / 100));
    updateTilt(current - 0.1);
  });

  document.getElementById('btn-tilt-up').addEventListener('click', () => {
    const current = tiltVal.textContent === 'Neutral' ? 0.0 : (tiltVal.textContent.startsWith('Gasoline') ? -parseFloat(tiltSlider.value / 100) : parseFloat(tiltSlider.value / 100));
    updateTilt(current + 0.1);
  });

  // Debt Slider Logic
  const debtSlider = document.getElementById('slider-debt-adjust');
  debtSlider.addEventListener('change', () => {
    if (!gameView) return;
    const maxDebt = gameView.debt + gameView.borrowing_capacity;
    const targetDebt = (debtSlider.value / 100) * maxDebt;
    const diff = targetDebt - gameView.debt;
    
    if (diff > 1000) {
      pendingActions.push({ Borrow: Math.round(diff) });
    } else if (diff < -1000) {
      pendingActions.push({ Repay: Math.round(-diff) });
    }
  });

  // Quick Debt Buttons
  document.getElementById('btn-borrow-quick').addEventListener('click', () => {
    pendingActions.push({ Borrow: 20_000_000 });
  });
  document.getElementById('btn-repay-quick').addEventListener('click', () => {
    pendingActions.push({ Repay: 20_000_000 });
  });

  // Restartcampaign
  document.getElementById('btn-restart').addEventListener('click', () => {
    location.reload();
  });
}

function updateDebtSliderState(view) {
  const debtSlider = document.getElementById('slider-debt-adjust');
  const maxDebt = view.debt + view.borrowing_capacity;
  if (maxDebt > 0) {
    debtSlider.value = Math.round((view.debt / maxDebt) * 100);
  } else {
    debtSlider.value = 0;
  }
}

// ── Rendering Manager ──────────────────────────────────────────────────────
function renderAll(view) {
  renderHeader(view);
  renderSchematic(view);
  renderMarket(view);
  renderPnl(view);
  renderProducts(view);
  renderUnits(view);
  renderMaintenance(view);
  renderProjects(view);
  renderEvents(view);
  renderChart(view);
}

// ── Formatters ─────────────────────────────────────────────────────────────
function formatMoney(n) {
  const abs = Math.abs(n);
  const sign = n < 0 ? '-' : '';
  if (abs >= 1e9) return sign + '£' + (abs / 1e9).toFixed(2) + 'B';
  if (abs >= 1e6) return sign + '£' + (abs / 1e6).toFixed(1) + 'M';
  if (abs >= 1e3) return sign + '£' + (abs / 1e3).toFixed(0) + 'K';
  return sign + '£' + abs.toFixed(0);
}

function formatNum(n, dp = 0) {
  if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (n >= 1e3) return (n / 1e3).toFixed(dp) + 'K';
  return n.toFixed(dp);
}

// ── Render Top Telemetry Header ────────────────────────────────────────────
function renderHeader(view) {
  document.getElementById('stat-week').textContent = view.week;
  
  // Render Values
  document.getElementById('stat-cash').textContent = formatMoney(view.cash);
  document.getElementById('stat-valuation').textContent = formatMoney(view.valuation);
  document.getElementById('stat-debt').textContent = formatMoney(view.debt);

  // Win percentage calculation
  const winPct = Math.min(100, (view.valuation / view.victory_target) * 100);
  document.getElementById('valuation-bar').style.width = winPct + '%';
  document.getElementById('valuation-pct').textContent = winPct.toFixed(0) + '%';

  // Render Trends
  renderTrendElement('trend-cash', view.cash, prevStats.cash);
  renderTrendElement('trend-valuation', view.valuation, prevStats.valuation);
  renderTrendElement('trend-debt', view.debt, prevStats.debt, true); // true = debt going up is negative trend
}

function renderTrendElement(id, current, prev, inverse = false) {
  const el = document.getElementById(id);
  if (prev === null || Math.abs(current - prev) < 100) {
    el.innerHTML = '--';
    el.className = 'telemetry-trend mono trend-neutral';
    return;
  }

  const diff = current - prev;
  const isUp = diff > 0;
  const isPositiveTrend = inverse ? !isUp : isUp;
  
  const sign = isUp ? '▲' : '▼';
  const prefix = isUp ? '+' : '-';
  
  el.innerHTML = `${sign} ${prefix}${formatNum(Math.abs(diff), 1)}`;
  el.className = `telemetry-trend mono ${isPositiveTrend ? 'trend-up' : 'trend-down'}`;
}

// ── Render SVG Schematic (The Tactical Board) ──────────────────────────────
function renderSchematic(view) {
  // 1. Update Crude Supply Nodes
  document.getElementById('schematic-crude-price').textContent = `£${view.market.crude_price.toFixed(2)}`;

  // 2. Extract Volumes
  const gasoline = view.products.find(p => p.name === 'gasoline');
  const diesel = view.products.find(p => p.name === 'diesel');
  const gasolVol = gasoline ? gasoline.volume : 0;
  const dieselVol = diesel ? diesel.volume : 0;

  document.getElementById('schematic-gasoline-val').textContent = `${formatNum(gasolVol)} bbl/d`;
  document.getElementById('schematic-diesel-val').textContent = `${formatNum(dieselVol)} bbl/d`;

  // 3. Extract Dispositions / Byproducts
  // Find crude outputs to estimate byproduct volume
  // In our model, LPG and Residue yields are mapped to streams
  // Let's grab total LPG and Residue volumes sold in P&L
  // LPG is idx 5, Residue is idx 2, Coke is idx 6 (slop)
  // Let's look up byproducts via product view blends or view sales
  // Wait, we can estimate byproduct volumes using stream idx values
  // Let's search view.revenue/crude_charge or products details.
  // Actually, we can get LPG, Residue, Coke rates directly from P&L sales list!
  // In solve.rs: `sales` contains tuples of (stream_name, bbl_day volume)
  // But wait, does GameView expose sales?
  // Let's check state.rs: GameView has `weekly_margin`, `crude_charge`, etc., but does it expose sales?
  // No, but we can compute or read them!
  // Wait, in state.rs, LPG and Residue are dispositions. Let's lookup them in products.
  // If not explicitly exposed as a direct field, we can fallback to displaying crude fractions!
  // Let's check what variables are exposed inside `app.js` before our rewrite:
  // In old app.js: LPG, Residue, Coke were not explicitly drawn, we added them in index.html.
  // Let's check how we can estimate them:
  // ADU capacity is 100K bbl/d. Yield splits: Naphtha (25%), Gasoil (45%), Residue (30%).
  // So Residue volume = ADU throughput * 0.3.
  // FCC capacity is 50K. High/low severity yields split LPG, Coke, fcc_gaso, LCO.
  // We can calculate estimates of LPG, Residue, Coke based on the unit throughputs!
  // ADU unit: view.units[0].
  // FCC unit: view.units[1].
  const adu = view.units.find(u => u.name === 'ADU') || view.units[0];
  const fcc = view.units.find(u => u.name === 'FCC') || view.units[1];
  
  const aduThroughput = adu ? adu.throughput : 0;
  const fccThroughput = fcc ? fcc.throughput : 0;
  
  const residueEst = aduThroughput * 0.30;
  // FCC LPG yield: Low severity = 6%, High severity = 10%. Average = 8%.
  const fccSev = fcc && fcc.realised_severity != null ? fcc.realised_severity : 0.5;
  const lpgYield = 0.06 + (fccSev * 0.04);
  const lpgEst = fccThroughput * lpgYield;
  // FCC Coke yield: Low = 5%, High = 8%.
  const cokeYield = 0.05 + (fccSev * 0.03);
  const cokeEst = fccThroughput * cokeYield;

  document.getElementById('schematic-lpg-val').textContent = `LPG: ${formatNum(lpgEst)} bbl/d`;
  document.getElementById('schematic-residue-val').textContent = `Residue: ${formatNum(residueEst)} bbl/d`;
  document.getElementById('schematic-coke-val').textContent = `Coke: ${formatNum(cokeEst)} bbl/d`;

  if (fcc) {
    const fccUtil = fcc.capacity > 0 ? (fcc.throughput / fcc.capacity * 100) : 0;
    document.getElementById('schematic-fcc-util').textContent = `${fccUtil.toFixed(0)}% Util`;
  }

  // 4. Update Node Status Classes (Bottleneck, Turnaround, Tripped)
  updateSchematicNodeClass('node-adu', 'ADU', view);
  updateSchematicNodeClass('node-fcc', 'FCC', view);

  // 5. Update flow speed animations
  setPipeFlowSpeed('flow-crude', aduThroughput, 100000);
  setPipeFlowSpeed('flow-naphtha', aduThroughput * 0.25, 25000);
  setPipeFlowSpeed('flow-gasoil-to-diesel', aduThroughput * 0.45 - fccThroughput, 45000);
  setPipeFlowSpeed('flow-gasoil-to-fcc', fccThroughput, 50000);
  setPipeFlowSpeed('flow-residue', residueEst, 30000);

  const fccGasoVol = fccThroughput * (0.50 + fccSev * 0.08);
  const lcoVol = fccThroughput * (0.30 - fccSev * 0.12);
  setPipeFlowSpeed('flow-fcc-gaso', fccGasoVol, 29000);
  setPipeFlowSpeed('flow-lco', lcoVol, 15000);
  setPipeFlowSpeed('flow-fcc-byproducts', lpgEst + cokeEst, 9000);
}

function updateSchematicNodeClass(svgId, unitName, view) {
  const node = document.getElementById(svgId);
  if (!node) return;

  const unit = view.units.find(u => u.name === unitName);
  const shadowPrice = (view.shadow_prices.find(([name]) => name === unitName) || [null, 0])[1];

  // Remove status classes
  node.classList.remove('bottleneck', 'tripped', 'turnaround');

  if (!unit) return;

  if (unit.maintenance_status === 'Turnaround') {
    node.classList.add('turnaround');
  } else if (unit.maintenance_status === 'Tripped!') {
    node.classList.add('tripped');
  } else if (shadowPrice > 0.05) {
    node.classList.add('bottleneck');
  }
}

function setPipeFlowSpeed(flowId, value, maxVal) {
  const path = document.getElementById(flowId);
  if (!path) return;

  if (value < 1.0) {
    path.style.animation = 'none';
    path.style.strokeOpacity = '0.1';
    return;
  }

  path.style.animation = ''; // restore animation
  path.style.strokeOpacity = '';

  const ratio = Math.min(1.0, value / maxVal);
  // Speed maps from 4s (very slow) at 0% flow to 0.8s (very fast) at 100% flow
  const duration = 4.0 - (ratio * 3.2);
  path.style.animationDuration = `${duration}s`;
}

// ── Render Market Prices & Spreads ─────────────────────────────────────────
function renderMarket(view) {
  const container = document.getElementById('market-content');
  if (!container) return;

  const brent = view.market.crude_price;
  const gasol = view.market.gasoline_price;
  const diesel = view.market.diesel_price;

  container.innerHTML = `
    <div class="market-feed-row">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">Brent Crude</span>
        <span class="market-feed-sublbl">Base Input Cost</span>
      </div>
      <span class="market-feed-val mono">£${brent.toFixed(2)}</span>
    </div>
    
    <div class="market-feed-row">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">Gasoline (Premium)</span>
        <span class="market-feed-sublbl">Finished Cut</span>
      </div>
      <span class="market-feed-val mono" style="color: var(--accent-green)">£${gasol.toFixed(2)}</span>
    </div>

    <div class="market-feed-row">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">Diesel (Low Sulfur)</span>
        <span class="market-feed-sublbl">Finished Cut</span>
      </div>
      <span class="market-feed-val mono" style="color: var(--accent-cyan)">£${diesel.toFixed(2)}</span>
    </div>

    <div class="market-feed-row spread">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">Gasoline Crack Spread</span>
        <span class="market-feed-sublbl">Refinery margin proxy</span>
      </div>
      <span class="market-feed-val mono" style="color: var(--accent-green)">
        +£${(gasol - brent).toFixed(2)}
      </span>
    </div>

    <div class="market-feed-row spread">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">Diesel Crack Spread</span>
        <span class="market-feed-sublbl">Refinery margin proxy</span>
      </div>
      <span class="market-feed-val mono" style="color: var(--accent-cyan)">
        +£${(diesel - brent).toFixed(2)}
      </span>
    </div>
  `;
}

// ── Render Finance P&L ─────────────────────────────────────────────────────
function renderPnl(view) {
  const container = document.getElementById('pnl-content');
  if (!container) return;

  const margin = view.weekly_margin;
  const deltaCash = margin - view.interest;
  const effPct = (view.execution_efficiency ?? 1) * 100;
  const effColor = effPct >= 97 ? '#00e676' : effPct >= 92 ? '#ffb300' : '#ff5252';

  container.innerHTML = `
    <div class="ledger-row">
      <span class="ledger-label">Execution vs Plan</span>
      <span class="ledger-value mono" style="color:${effColor}">${effPct.toFixed(0)}% of optimum</span>
    </div>
    <div class="ledger-row">
      <span class="ledger-label">Finished Product Sales</span>
      <span class="ledger-value credit">${formatMoney(view.revenue)}</span>
    </div>
    <div class="ledger-row">
      <span class="ledger-label">Raw Byproduct Sales</span>
      <span class="ledger-value credit">+£0.0K</span>
    </div>
    <div class="ledger-row">
      <span class="ledger-label">Crude Replacement Cost</span>
      <span class="ledger-value debit">-${formatMoney(view.crude_cost)}</span>
    </div>
    <div class="ledger-row">
      <span class="ledger-label">Variable Operating Costs</span>
      <span class="ledger-value debit">-${formatMoney(view.variable_opex)}</span>
    </div>
    <div class="ledger-row">
      <span class="ledger-label">Fixed Refinery Overheads</span>
      <span class="ledger-value debit">-${formatMoney(view.fixed_opex)}</span>
    </div>
    
    <div class="ledger-row total-section">
      <span class="ledger-label">Operating EBITDA Margin</span>
      <span class="ledger-value ${margin >= 0 ? 'credit' : 'debit'}">${formatMoney(margin)}</span>
    </div>

    <div class="ledger-row">
      <span class="ledger-label">Weekly Financing Interest</span>
      <span class="ledger-value debit">-${formatMoney(view.interest)}</span>
    </div>

    <div class="ledger-row grand-total">
      <span class="ledger-label">Net Weekly Cashflow (Δ Cash)</span>
      <span class="ledger-value ${deltaCash >= 0 ? 'credit' : 'debit'}">${formatMoney(deltaCash)}</span>
    </div>

    <div class="shadow-prices-box">
      <div class="shadow-price-title">Debottlenecking Shadow Prices</div>
      ${view.shadow_prices.map(([name, sp]) => `
        <div class="ledger-row" style="font-size:0.75rem; border-bottom:none; padding:4px 0;">
          <span class="ledger-label" style="padding-left: 8px;">↳ ${name} capacity value</span>
          <span class="ledger-value mono" style="color: ${sp > 0.01 ? 'var(--accent-amber)' : 'var(--text-muted)'}">
            £${sp.toFixed(2)} / bbl·d
          </span>
        </div>
      `).join('')}
    </div>
  `;
}

// ── Render Product Slate & Blend pool ──────────────────────────────────────
const BLEND_COLORS = [
  '#00e5ff', '#00e676', '#ffb300', '#d500f9', '#ff1744', '#64748b'
];

function renderProducts(view) {
  const container = document.getElementById('products-content');
  if (!container) return;

  container.innerHTML = view.products.map(p => {
    const totalVol = p.blend.reduce((sum, [, v]) => sum + v, 0);
    
    // Segment progress fills
    const blendBars = p.blend.map(([name, vol], idx) => {
      const pct = totalVol > 0 ? (vol / totalVol * 100) : 0;
      return `<div class="blend-bar-fill" style="width: ${pct}%; background: ${BLEND_COLORS[idx % BLEND_COLORS.length]}"></div>`;
    }).join('');

    // Detailed legend list
    const legend = p.blend.map(([name, vol], idx) => {
      const pct = totalVol > 0 ? (vol / totalVol * 100) : 0;
      return `
        <span class="blend-details-item">
          <span class="blend-bullet" style="background: ${BLEND_COLORS[idx % BLEND_COLORS.length]}"></span>
          <span class="mono">${name}: ${pct.toFixed(0)}%</span>
        </span>
      `;
    }).join('');

    return `
      <div class="product-card">
        <div class="product-card-header">
          <span class="product-card-name">${p.name}</span>
          <span class="product-card-meta green">£${p.price.toFixed(0)}/bbl</span>
        </div>
        <div class="blend-bar-track">${blendBars}</div>
        <div class="blend-details-list">${legend}</div>
        <div style="margin-top: 8px; font-size: 0.72rem; color: var(--text-secondary); text-align:right;" class="mono">
          Vol: ${formatNum(p.volume)} bbl/d
        </div>
      </div>
    `;
  }).join('');
}

// ── Render Engineering Dials (Health & Utilisation) ────────────────────────
function renderUnits(view) {
  const container = document.getElementById('units-content');
  if (!container) return;

  container.innerHTML = view.units.map(u => {
    const utilPct = Math.round(u.utilisation * 100);
    const healthPct = Math.round(u.health * 100);
    
    // SVG radial variables
    // Circumference = 2 * PI * r (r=28) = 176
    const circ = 176;
    const utilOffset = circ - (utilPct / 100 * circ);
    const healthOffset = circ - (healthPct / 100 * circ);

    // Health color categorisation
    let healthColorClass = 'health';
    if (healthPct < 30) healthColorClass = 'health-low';
    else if (healthPct < 65) healthColorClass = 'health-warning';

    // Badge styling
    let statusClass = 'running';
    let statusText = u.maintenance_status;
    if (u.maintenance_status === 'Turnaround') {
      statusClass = 'turnaround';
      statusText = `T/A (${u.maintenance_weeks_remaining}w)`;
    } else if (u.maintenance_status === 'Tripped!') {
      statusClass = 'tripped';
      statusText = `TRIP (${u.maintenance_weeks_remaining}w)`;
    }

    const severityInfo = u.realised_severity != null
      ? `<div style="font-size:0.75rem;"><span class="text-muted">Target Severity:</span> <span class="mono text-bright">${u.realised_severity.toFixed(3)}</span></div>`
      : '';

    // Stochastic outage hazard for this week — the visible reliability gamble.
    const riskPct = (u.outage_risk || 0) * 100;
    const riskColor = riskPct > 15 ? '#ff5252' : riskPct > 5 ? '#ffb300' : '#00e676';
    const riskInfo = `<div style="font-size:0.75rem;"><span class="text-muted">Outage risk/wk:</span> <span class="mono" style="color:${riskColor}">${riskPct.toFixed(1)}%</span></div>`;

    return `
      <div class="unit-dial-card">
        <div class="unit-dial-title-row">
          <span class="unit-dial-name">${u.name}</span>
          <span class="unit-dial-status-badge ${statusClass}">${statusText}</span>
        </div>
        
        <div class="dials-row">
          <!-- Utilisation Dial -->
          <div class="dial-wrapper">
            <svg class="dial-svg">
              <circle cx="40" cy="40" r="28" class="dial-bg"></circle>
              <circle cx="40" cy="40" r="28" class="dial-fill utilisation" 
                      style="stroke-dasharray: ${circ}; stroke-dashoffset: ${utilOffset}"></circle>
            </svg>
            <span class="dial-text digital">${utilPct}%</span>
            <span class="dial-label">Util</span>
          </div>

          <!-- Health Dial -->
          <div class="dial-wrapper">
            <svg class="dial-svg">
              <circle cx="40" cy="40" r="28" class="dial-bg"></circle>
              <circle cx="40" cy="40" r="28" class="dial-fill ${healthColorClass}" 
                      style="stroke-dasharray: ${circ}; stroke-dashoffset: ${healthOffset}"></circle>
            </svg>
            <span class="dial-text digital">${healthPct}%</span>
            <span class="dial-label">Health</span>
          </div>
        </div>

        <div class="unit-dial-meta-row">
          <div class="mono">Flow: ${formatNum(u.throughput)}/${formatNum(u.capacity)} bbl/d</div>
          ${severityInfo}
          ${riskInfo}
        </div>
      </div>
    `;
  }).join('');
}

// ── Render Turnaround Action Cards ──────────────────────────────────────────
function renderMaintenance(view) {
  const container = document.getElementById('maintenance-buttons');
  if (!container) return;

  container.innerHTML = view.units.map(u => {
    const isRunning = u.maintenance_status === 'Running';
    const btnClass = isRunning ? 'btn-warning' : 'btn-secondary';
    const disabledAttr = isRunning ? '' : 'disabled';
    const btnLabel = isRunning ? 'Schedule' : u.maintenance_status;
    const healthPct = (u.health * 100).toFixed(0);

    return `
      <div class="engineering-action-card">
        <div class="action-meta">
          <span class="action-meta-title">${u.name} Maintenance</span>
          <span class="action-meta-status">Integrity: ${healthPct}% — State: ${u.maintenance_status}</span>
        </div>
        <button class="btn btn-engineering ${btnClass}" ${disabledAttr}
                onclick="window.scheduleTurnaround('${u.name}')">
          ${btnLabel}
        </button>
      </div>
    `;
  }).join('');
}

// ── Render Capital Projects Card Deck ────────────────────────────────────────
function renderProjects(view) {
  const container = document.getElementById('projects-list');
  if (!container) return;

  // Active Projects
  const active = view.active_projects.map(p => `
    <div class="project-card" style="border-style: dashed; border-color: var(--accent-purple)">
      <div class="project-card-header">
        <span class="project-title">${p.name}</span>
        <span class="project-gain-badge" style="color:var(--accent-purple); border-color:var(--accent-purple); background:rgba(213,0,249,0.1)">
          +${formatNum(p.capacity_gain)} bbl/d
        </span>
      </div>
      <p class="project-desc">Construction crew debottlenecking ${p.unit_name}.</p>
      <div class="project-footer">
        <span class="mono" style="color: var(--accent-purple); font-size: 0.8rem;">
          In Construction (${p.weeks_remaining}w left)
        </span>
      </div>
    </div>
  `).join('');

  // Available Projects
  const available = view.available_projects.map(p => `
    <div class="project-card">
      <div class="project-card-header">
        <span class="project-title">${p.name}</span>
        <span class="project-gain-badge">+${formatNum(p.capacity_gain)} bbl/d</span>
      </div>
      <p class="project-desc">${p.description || `Expand capacity of ${p.unit_name} to raise production ceiling.`}</p>
      <div class="project-footer">
        <span class="project-cost">${formatMoney(p.cost)}</span>
        <button class="btn btn-success btn-secondary"
                onclick="window.approveProject(${p.config_index})">
          APPROVE (Takes ${p.duration_weeks}w)
        </button>
      </div>
    </div>
  `).join('');

  const contentHtml = active + available;
  container.innerHTML = contentHtml || `<div style="color: var(--text-muted); font-size: 0.85rem; padding: 20px 0;">No active or available construction contracts.</div>`;
}

// ── Render Corporate Finance Ledger details ────────────────────────────────
function renderFinance(view) {
  const container = document.getElementById('finance-content');
  if (!container) return;

  const totalFacility = view.debt + view.borrowing_capacity;
  const leveragePct = totalFacility > 0 ? (view.debt / totalFacility * 100) : 0;

  container.innerHTML = `
    <div class="fin-tel-card">
      <span class="fin-tel-lbl">Liquid Cash Reserve</span>
      <span class="fin-tel-val mono" style="color: var(--accent-green)">${formatMoney(view.cash)}</span>
    </div>
    <div class="fin-tel-card leverage">
      <span class="fin-tel-lbl">Facility Utilisation</span>
      <span class="fin-tel-val mono">${leveragePct.toFixed(0)}% (${formatMoney(view.debt)})</span>
    </div>
    <div class="fin-tel-card">
      <span class="fin-tel-lbl">Debt Draw Room</span>
      <span class="fin-tel-val mono" style="color: var(--accent-amber)">${formatMoney(view.borrowing_capacity)}</span>
    </div>
    <div class="fin-tel-card equity">
      <span class="fin-tel-lbl">Company Net Equity</span>
      <span class="fin-tel-val mono">${formatMoney(view.equity)}</span>
    </div>
  `;
}

// ── Performance Ticker (Event Log) ─────────────────────────────────────────
function renderEvents(view) {
  const container = document.getElementById('events-content');
  if (!container) return;

  // Append new events
  const newEvents = view.events.slice(lastEventCount);
  lastEventCount = view.events.length;

  for (const evt of newEvents) {
    const el = document.createElement('div');
    const sevClass = evt.severity === 'Critical' ? 'critical' : evt.severity === 'Warning' ? 'warning' : '';
    el.className = `event-item ${sevClass}`;
    el.innerHTML = `
      <span class="event-week mono">WEEK ${evt.week}</span>
      <span class="event-message">${evt.message}</span>
    `;
    container.prepend(el); // newest at the top
  }

  // Cap DOM children size to 100 entries
  while (container.children.length > 100) {
    container.removeChild(container.lastChild);
  }
}

// ── Render Charts ──────────────────────────────────────────────────────────
function renderChart(view) {
  const canvas = document.getElementById('chart-history');
  if (!canvas || document.getElementById('tab-finance').classList.contains('hidden')) return;
  drawHistoryChart(canvas, view.history, view.victory_target);
}

// ── Game Over Panel overlay ────────────────────────────────────────────────
function showGameOver(status) {
  const overlay = document.getElementById('game-over');
  const title = document.getElementById('game-over-title');
  const msg = document.getElementById('game-over-message');

  overlay.classList.remove('hidden');

  if (status.Won) {
    title.textContent = '🏆 SIMULATION COMPLETE';
    title.className = 'overlay-title victory';
    msg.textContent = `Excellent job! You successfully hit the board's EV valuation target in Week ${status.Won.week}, securing a final valuation of ${formatMoney(gameView.valuation)}.`;
  } else if (status.Lost) {
    title.textContent = '💀 BANKRUPTCY & INSOLVENCY';
    title.className = 'overlay-title defeat';
    msg.textContent = `Campaign terminated: ${status.Lost.reason} in Week ${status.Lost.week}. The credit facility has been liquidated.`;
  }
}

// ── Global Actions (Exposed to dynamic click bindings) ─────────────────────
window.scheduleTurnaround = (unitName) => {
  pendingActions.push({ ScheduleTurnaround: unitName });
  doTick(); // Quick tick to provide immediate visual feedback of turnaround schedule
};

window.approveProject = (configIndex) => {
  pendingActions.push({ ApproveProject: configIndex });
  doTick(); // Quick tick to immediately reflect construction budget deduction
};

// ── Sidebar pin / collapse handler ──────────────────────────────────────────
let sidebarPinned = true;
let sidebarCollapsed = false;

function setupSidebarBehavior() {
  const sidebar = document.getElementById('main-sidebar');
  const appLayout = document.querySelector('.app-layout');
  const btnPin = document.getElementById('btn-pin-sidebar');
  const btnToggle = document.getElementById('btn-toggle-sidebar');

  // Load configuration from local storage
  sidebarPinned = localStorage.getItem('sidebar_pinned') !== 'false';
  sidebarCollapsed = localStorage.getItem('sidebar_collapsed') === 'true';

  const updateSidebarState = () => {
    // Clear layout classes
    appLayout.classList.remove(
      'sidebar-pinned-expanded',
      'sidebar-pinned-collapsed',
      'sidebar-unpinned-collapsed',
      'sidebar-unpinned-expanded'
    );
    sidebar.classList.remove('collapsed', 'unpinned');

    // Update icons
    btnPin.classList.toggle('pinned', sidebarPinned);
    btnPin.textContent = sidebarPinned ? '📌' : '📍';
    btnToggle.textContent = sidebarCollapsed ? '▶' : '◀';

    if (sidebarPinned) {
      if (sidebarCollapsed) {
        appLayout.classList.add('sidebar-pinned-collapsed');
        sidebar.classList.add('collapsed');
      } else {
        appLayout.classList.add('sidebar-pinned-expanded');
      }
    } else {
      sidebar.classList.add('unpinned');
      if (sidebarCollapsed) {
        appLayout.classList.add('sidebar-unpinned-collapsed');
        sidebar.classList.add('collapsed');
      } else {
        appLayout.classList.add('sidebar-unpinned-expanded');
      }
    }

    localStorage.setItem('sidebar_pinned', sidebarPinned);
    localStorage.setItem('sidebar_collapsed', sidebarCollapsed);
  };

  btnPin.addEventListener('click', (e) => {
    e.stopPropagation();
    sidebarPinned = !sidebarPinned;
    updateSidebarState();
  });

  btnToggle.addEventListener('click', (e) => {
    e.stopPropagation();
    sidebarCollapsed = !sidebarCollapsed;
    updateSidebarState();
  });

  // Slider expansion overlay behavior on hover for unpinned state
  sidebar.addEventListener('mouseenter', () => {
    if (!sidebarPinned && sidebarCollapsed) {
      sidebar.classList.remove('collapsed');
      btnToggle.textContent = '◀';
    }
  });

  sidebar.addEventListener('mouseleave', () => {
    if (!sidebarPinned && sidebarCollapsed) {
      sidebar.classList.add('collapsed');
      btnToggle.textContent = '▶';
    }
  });

  // Automatically collapse sidebar on navigation tap when unpinned
  const navBtns = document.querySelectorAll('.nav-btn');
  navBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      if (!sidebarPinned) {
        sidebarCollapsed = true;
        updateSidebarState();
      }
    });
  });

  updateSidebarState();
}

// ── Run Campaign ────────────────────────────────────────────────────────────
init();
