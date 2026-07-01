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
let lastWeekEvents = [];    // events that fired in the most recent tick (for the briefing)

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
    // Record current stats to calculate trends + weekly-briefing deltas
    const prevEventCount = gameView ? gameView.events.length : 0;
    if (gameView) {
      prevStats.cash = gameView.cash;
      prevStats.valuation = gameView.valuation;
      prevStats.debt = gameView.debt;
    }

    gameView = game.tick(actionsJson);
    lastWeekEvents = gameView.events.slice(prevEventCount); // just this week's
    renderAll(gameView);

    // Reset debt slider + pending indicator to reflect the new state
    updateDebtSliderState(gameView);
    updatePendingBadge();

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

// ── Deliberate turn loop ────────────────────────────────────────────────────
// Advance exactly one week, then surface a briefing of what happened. This is the
// primary way to play; auto-run (play/fast) is for skipping ahead.
function manualAdvance() {
  setSpeed(0);
  if (!game || !game.is_running()) return;
  doTick();
  if (gameView && gameView.status === 'Running') showBriefing(gameView);
}

// Queue a player action for the next advance instead of acting immediately. Slider
// settings replace any earlier value of the same kind so the queue stays clean.
function queueAction(action) {
  const key = Object.keys(action)[0];
  if (key === 'SetSeverity' || key === 'SetProductTilt') {
    pendingActions = pendingActions.filter(a => !(key in a));
  }
  pendingActions.push(action);
  updatePendingBadge();
}

function updatePendingBadge() {
  const badge = document.getElementById('pending-badge');
  if (!badge) return;
  const n = pendingActions.length;
  badge.textContent = n ? ` · ${n} order${n > 1 ? 's' : ''}` : '';
  badge.classList.toggle('hidden', n === 0);
}

function hideBriefing() {
  document.getElementById('weekly-briefing').classList.add('hidden');
}

function showBriefing(view) {
  const dEv = view.valuation - (prevStats.valuation ?? view.valuation);
  const dCash = view.cash - (prevStats.cash ?? view.cash);
  const netCf = view.weekly_margin - view.interest;
  const pct = Math.min(100, (view.valuation / view.victory_target) * 100);
  const effPct = (view.execution_efficiency ?? 1) * 100;

  document.getElementById('briefing-week').textContent = `WEEK ${view.week}`;
  document.getElementById('briefing-evpct').textContent = `${pct.toFixed(0)}% to £500M target`;
  document.getElementById('briefing-next').textContent = view.week + 1;

  const stat = (label, value, delta, good) => {
    const arrow = delta == null ? '' : delta > 0 ? '▲' : delta < 0 ? '▼' : '';
    const dColor = delta == null || Math.abs(delta) < 1 ? 'var(--text-muted,#888)'
      : (good ? '#00e676' : '#ff5252');
    const dTxt = delta == null ? '' : ` <span style="color:${dColor}">${arrow} ${formatMoney(Math.abs(delta))}</span>`;
    return `<div class="briefing-stat" style="display:flex;justify-content:space-between;padding:6px 0;border-bottom:1px solid rgba(255,255,255,0.06);">
      <span style="color:var(--text-muted,#999)">${label}</span>
      <span class="mono">${value}${dTxt}</span></div>`;
  };

  document.getElementById('briefing-stats').innerHTML =
    stat('Enterprise Value', formatMoney(view.valuation), dEv, dEv >= 0) +
    stat('Cash', formatMoney(view.cash), dCash, dCash >= 0) +
    stat('Net cashflow', formatMoney(netCf), null) +
    stat('Operating EBITDA', formatMoney(view.weekly_margin), null) +
    `<div class="briefing-stat" style="display:flex;justify-content:space-between;padding:6px 0;">
      <span style="color:var(--text-muted,#999)">Execution vs plan</span>
      <span class="mono" style="color:${effPct >= 97 ? '#00e676' : effPct >= 92 ? '#ffb300' : '#ff5252'}">${effPct.toFixed(0)}%</span></div>`;

  const evHtml = lastWeekEvents.length
    ? lastWeekEvents.map(e => {
        const c = e.severity === 'Critical' ? '#ff5252' : e.severity === 'Warning' ? '#ffb300' : 'var(--accent-cyan,#00e5ff)';
        return `<div style="padding:6px 8px;margin-top:6px;border-left:3px solid ${c};background:rgba(255,255,255,0.03);font-size:0.85rem;">${e.message}</div>`;
      }).join('')
    : `<div style="padding:8px;color:var(--text-muted,#888);font-size:0.85rem;">A quiet week — operations nominal.</div>`;
  document.getElementById('briefing-events').innerHTML =
    `<div style="margin-top:14px;font-size:0.7rem;letter-spacing:0.1em;color:var(--text-muted,#888)">THIS WEEK</div>${evHtml}`;

  document.getElementById('weekly-briefing').classList.remove('hidden');
}

// ── Controls & Actions ──────────────────────────────────────────────────────
function setupControls() {
  // Speed buttons
  document.getElementById('btn-pause').addEventListener('click', () => setSpeed(0));
  document.getElementById('btn-play').addEventListener('click', () => setSpeed(1));
  document.getElementById('btn-fast').addEventListener('click', () => setSpeed(5));
  
  document.getElementById('btn-step').addEventListener('click', manualAdvance);

  // Weekly briefing: Continue advances the next week; Review just closes it.
  document.getElementById('btn-briefing-continue').addEventListener('click', () => {
    hideBriefing();
    manualAdvance();
  });
  document.getElementById('btn-briefing-close').addEventListener('click', hideBriefing);

  // Severity controls (+/- adjust)
  const sevSlider = document.getElementById('slider-severity');
  const sevVal = document.getElementById('severity-value');
  
  const updateSeverity = (val) => {
    val = Math.max(0.0, Math.min(1.0, val));
    sevSlider.value = Math.round(val * 100);
    sevVal.textContent = val.toFixed(2);
    queueAction({ SetSeverity: val });
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
    queueAction({ SetProductTilt: val });
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
      queueAction({ Borrow: Math.round(diff) });
    } else if (diff < -1000) {
      queueAction({ Repay: Math.round(-diff) });
    }
  });

  // Quick Debt Buttons
  document.getElementById('btn-borrow-quick').addEventListener('click', () => {
    queueAction({ Borrow: 20_000_000 });
  });
  document.getElementById('btn-repay-quick').addEventListener('click', () => {
    queueAction({ Repay: 20_000_000 });
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
  document.getElementById('schematic-crude-price').textContent = `£${view.market.crude_price.toFixed(2)}`;

  // Real solved flows straight from the LP — no hardcoded yields.
  const flow = new Map((view.stream_production || []).map(([n, v]) => [n, v]));
  const prod = (n) => flow.get(n) || 0;

  const gasolVol = view.products.find(p => p.name === 'gasoline')?.volume || 0;
  const dieselVol = view.products.find(p => p.name === 'diesel')?.volume || 0;
  document.getElementById('schematic-gasoline-val').textContent = `${formatNum(gasolVol)} bbl/d`;
  document.getElementById('schematic-diesel-val').textContent = `${formatNum(dieselVol)} bbl/d`;

  const fcc = view.units.find(u => u.name === 'FCC');
  const aduThroughput = view.crude_charge || 0;
  const fccThroughput = fcc ? fcc.throughput : 0;
  const gasoilTotal = prod('gasoil') + prod('gasoil_hs');

  const residue = prod('residue');
  const lpg = prod('lpg');
  const coke = prod('coke');
  document.getElementById('schematic-lpg-val').textContent = `LPG: ${formatNum(lpg)} bbl/d`;
  document.getElementById('schematic-residue-val').textContent = `Residue: ${formatNum(residue)} bbl/d`;
  document.getElementById('schematic-coke-val').textContent = `Coke: ${formatNum(coke)} bbl/d`;

  if (fcc) {
    const fccUtil = fcc.capacity > 0 ? (fcc.throughput / fcc.capacity * 100) : 0;
    document.getElementById('schematic-fcc-util').textContent = `${fccUtil.toFixed(0)}% Util`;
  }

  updateSchematicNodeClass('node-adu', 'ADU', view);
  updateSchematicNodeClass('node-fcc', 'FCC', view);

  // Flow-speed animations keyed to actual throughput.
  setPipeFlowSpeed('flow-crude', aduThroughput, 100000);
  setPipeFlowSpeed('flow-naphtha', prod('naphtha'), 30000);
  setPipeFlowSpeed('flow-gasoil-to-diesel', Math.max(0, gasoilTotal - fccThroughput), 45000);
  setPipeFlowSpeed('flow-gasoil-to-fcc', fccThroughput, 50000);
  setPipeFlowSpeed('flow-residue', residue, 45000);
  setPipeFlowSpeed('flow-fcc-gaso', prod('fcc_gaso'), 29000);
  setPipeFlowSpeed('flow-lco', prod('lco'), 15000);
  setPipeFlowSpeed('flow-fcc-byproducts', lpg + coke, 9000);
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

  // Crude grades: show each grade's price and mark the one(s) the plant is running.
  const running = new Set((view.crude_mix || []).map(([n]) => n));
  const crudeRows = (view.crude_prices || []).map(([name, price]) => {
    const on = running.has(name);
    return `<div class="market-feed-row">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">${name}${on ? ' <span style="color:#00e676">● running</span>' : ''}</span>
        <span class="market-feed-sublbl">Crude grade</span>
      </div>
      <span class="market-feed-val mono" style="${on ? 'color:#00e676' : 'opacity:0.7'}">£${price.toFixed(2)}</span>
    </div>`;
  }).join('');

  container.innerHTML = `
    <div class="market-feed-row">
      <div class="market-feed-meta">
        <span class="market-feed-lbl">Brent Crude</span>
        <span class="market-feed-sublbl">Benchmark</span>
      </div>
      <span class="market-feed-val mono">£${brent.toFixed(2)}</span>
    </div>
    ${crudeRows}

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

  // Hide units that aren't built yet (capacity 0 — e.g. the hydrocracker pre-project).
  container.innerHTML = view.units.filter(u => u.capacity > 0).map(u => {
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

  // Available Projects — with the TEA (NPV/IRR/payback) appraisal at forecast prices.
  const available = view.available_projects.map(p => {
    const good = p.npv > 0;
    const irrTxt = Number.isFinite(p.irr) ? `${(p.irr * 100).toFixed(0)}%` : '—';
    const payTxt = Number.isFinite(p.payback_years) ? `${p.payback_years.toFixed(1)}yr` : 'never';
    const col = good ? '#00e676' : '#ff5252';
    return `
    <div class="project-card">
      <div class="project-card-header">
        <span class="project-title">${p.name}</span>
        <span class="project-gain-badge">+${formatNum(p.capacity_gain)} bbl/d</span>
      </div>
      <p class="project-desc">${p.description || `Expand capacity of ${p.unit_name}.`}</p>
      <div class="tea-appraisal" style="display:flex;gap:12px;font-size:0.72rem;margin:6px 0;padding:6px 8px;background:rgba(255,255,255,0.03);border-left:3px solid ${col};">
        <span>NPV <b style="color:${col}" class="mono">${formatMoney(p.npv)}</b></span>
        <span>IRR <b class="mono">${irrTxt}</b></span>
        <span>Payback <b class="mono">${payTxt}</b></span>
      </div>
      <div class="project-footer">
        <span class="project-cost">${formatMoney(p.cost)}</span>
        <button class="btn btn-success btn-secondary"
                onclick="window.approveProject(${p.config_index})">
          APPROVE (Takes ${p.duration_weeks}w)
        </button>
      </div>
    </div>`;
  }).join('');

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
// Actions queue for the next advance — they do not burn a week on their own.
window.scheduleTurnaround = (unitName) => {
  queueAction({ ScheduleTurnaround: unitName });
};

window.approveProject = (configIndex) => {
  queueAction({ ApproveProject: configIndex });
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
