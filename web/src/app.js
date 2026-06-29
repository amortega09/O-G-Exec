/**
 * app.js — Main game controller for O&G Exec.
 *
 * Loads the WASM simulation engine, creates a Game instance from the tutorial
 * scenario, and drives the tick loop + UI updates.
 *
 * Architecture: WASM owns all game state. JS only does rendering. Every tick,
 * WASM returns a GameView object that JS renders into the DOM panels.
 */

import { drawHistoryChart } from './charts.js';

// ── State ───────────────────────────────────────────────────────────────────
let game = null;           // WASM Game instance
let gameView = null;       // Latest GameView from WASM
let victoryTarget = 500_000_000;
let tickSpeed = 0;         // 0=paused, 1=1×, 5=5×, -1=step
let tickTimer = null;
let pendingActions = [];
let lastEventCount = 0;

// Tick intervals by speed setting (ms between ticks)
const TICK_INTERVALS = { 1: 500, 5: 100, 10: 30 };

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

    // Parse config to get victory target
    const config = JSON.parse(scenarioJson);
    victoryTarget = config.victory_valuation || 500_000_000;

    // Create game
    const seed = BigInt(Date.now());
    game = new wasm.Game(scenarioJson, refineryJson, seed);

    // Get initial view
    gameView = game.view();

    // Wire up UI
    setupControls();
    renderAll(gameView);

    // Hide loading, show dashboard
    const loading = document.getElementById('loading-screen');
    loading.classList.add('fade-out');
    setTimeout(() => {
      loading.classList.add('hidden');
      document.getElementById('dashboard').classList.remove('hidden');
    }, 400);

    // Start paused
    setSpeed(0);

  } catch (err) {
    console.error('Failed to initialise:', err);
    document.querySelector('.loading-subtitle').textContent =
      `Error: ${err.message}. Check console.`;
  }
}

// ── Game Loop ───────────────────────────────────────────────────────────────
function doTick() {
  if (!game || !game.is_running()) return;

  const actionsJson = JSON.stringify(pendingActions);
  pendingActions = [];

  try {
    gameView = game.tick(actionsJson);
    renderAll(gameView);

    // Check for game over
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

  // Update button states
  document.getElementById('btn-pause').classList.toggle('active', speed === 0);
  document.getElementById('btn-play').classList.toggle('active', speed === 1);
  document.getElementById('btn-fast').classList.toggle('active', speed >= 5);

  if (speed > 0) {
    const interval = TICK_INTERVALS[speed] || 500;
    tickTimer = setInterval(doTick, interval);
  }
}

// ── Controls ────────────────────────────────────────────────────────────────
function setupControls() {
  // Speed controls
  document.getElementById('btn-pause').addEventListener('click', () => setSpeed(0));
  document.getElementById('btn-play').addEventListener('click', () => setSpeed(1));
  document.getElementById('btn-fast').addEventListener('click', () => setSpeed(5));
  document.getElementById('btn-step').addEventListener('click', () => {
    setSpeed(0);
    doTick();
  });

  // Severity slider
  const sevSlider = document.getElementById('slider-severity');
  const sevValue = document.getElementById('severity-value');
  sevSlider.addEventListener('input', () => {
    const v = sevSlider.value / 100;
    sevValue.textContent = v.toFixed(2);
    pendingActions.push({ SetSeverity: v });
  });

  // Product tilt slider
  const tiltSlider = document.getElementById('slider-tilt');
  tiltSlider.addEventListener('input', () => {
    const v = tiltSlider.value / 100;
    pendingActions.push({ SetProductTilt: v });
  });

  // Restart button
  document.getElementById('btn-restart').addEventListener('click', () => {
    location.reload();
  });
}

// ── Rendering ───────────────────────────────────────────────────────────────
function renderAll(view) {
  renderHeader(view);
  renderUnits(view);
  renderPnl(view);
  renderMarket(view);
  renderProducts(view);
  renderMaintenance(view);
  renderProjects(view);
  renderEvents(view);
  renderChart(view);
}

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

// ── Header ──────────────────────────────────────────────────────────────────
function renderHeader(view) {
  document.getElementById('stat-week').textContent = view.week;
  document.getElementById('stat-cash').textContent = formatMoney(view.cash);
  document.getElementById('stat-valuation').textContent = formatMoney(view.valuation);

  const pct = Math.min(100, (view.valuation / view.victory_target) * 100);
  document.getElementById('valuation-bar').style.width = pct + '%';
  document.getElementById('valuation-pct').textContent = pct.toFixed(0) + '%';
}

// ── Refinery Status ─────────────────────────────────────────────────────────
function renderUnits(view) {
  const container = document.getElementById('units-content');
  container.innerHTML = view.units.map(u => {
    const utilPct = (u.utilisation * 100).toFixed(0);
    const healthPct = (u.health * 100).toFixed(0);
    const healthClass = u.health < 0.3 ? 'health-low' : 'health';

    let statusClass = 'running';
    let statusText = u.maintenance_status;
    if (u.maintenance_status === 'Turnaround') {
      statusClass = 'turnaround';
      statusText = `TA (${u.maintenance_weeks_remaining}w)`;
    } else if (u.maintenance_status === 'Tripped!') {
      statusClass = 'tripped';
      statusText = `TRIP (${u.maintenance_weeks_remaining}w)`;
    }

    const sevLine = u.realised_severity != null
      ? `<div class="bar-row">
           <span class="bar-label">Severity</span>
           <span class="bar-value">${u.realised_severity.toFixed(3)}</span>
         </div>`
      : '';

    return `
      <div class="unit-card">
        <div class="unit-header">
          <span class="unit-name">${u.name}</span>
          <span class="unit-status ${statusClass}">${statusText}</span>
        </div>
        <div class="unit-bars">
          <div class="bar-row">
            <span class="bar-label">Util</span>
            <div class="bar-track">
              <div class="bar-fill utilisation" style="width: ${utilPct}%"></div>
            </div>
            <span class="bar-value">${formatNum(u.throughput)}/${formatNum(u.capacity)}</span>
          </div>
          <div class="bar-row">
            <span class="bar-label">Health</span>
            <div class="bar-track">
              <div class="bar-fill ${healthClass}" style="width: ${healthPct}%"></div>
            </div>
            <span class="bar-value">${healthPct}%</span>
          </div>
          ${sevLine}
        </div>
      </div>
    `;
  }).join('');
}

// ── P&L ─────────────────────────────────────────────────────────────────────
function renderPnl(view) {
  const container = document.getElementById('pnl-content');
  const margin = view.weekly_margin;
  const marginClass = margin >= 0 ? 'pnl-positive' : 'pnl-negative';

  container.innerHTML = `
    <div class="pnl-row">
      <span class="pnl-label">Revenue</span>
      <span class="pnl-value pnl-positive">${formatMoney(view.revenue)}</span>
    </div>
    <div class="pnl-row">
      <span class="pnl-label">Crude Cost</span>
      <span class="pnl-value pnl-negative">-${formatMoney(view.crude_cost)}</span>
    </div>
    <div class="pnl-row">
      <span class="pnl-label">Crude Charge</span>
      <span class="pnl-value">${formatNum(view.crude_charge)} bbl/d</span>
    </div>
    <div class="pnl-row total">
      <span class="pnl-label">Weekly Margin</span>
      <span class="pnl-value ${marginClass}">${formatMoney(margin)}</span>
    </div>
    <div class="pnl-row" style="margin-top: 8px;">
      <span class="pnl-label">Shadow Prices</span>
      <span class="pnl-value" style="font-size: 0.75rem; color: var(--text-muted)">£/bbl·day</span>
    </div>
    ${view.shadow_prices.map(([name, sp]) => `
      <div class="pnl-row">
        <span class="pnl-label" style="padding-left: 12px">${name}</span>
        <span class="pnl-value" style="color: ${sp > 0.01 ? 'var(--accent-amber)' : 'var(--text-muted)'}">${sp.toFixed(3)}</span>
      </div>
    `).join('')}
  `;
}

// ── Market ──────────────────────────────────────────────────────────────────
function renderMarket(view) {
  const container = document.getElementById('market-content');
  container.innerHTML = `
    <div class="market-row">
      <span class="market-name">Brent Crude</span>
      <span class="market-price">£${view.market.crude_price.toFixed(2)}</span>
    </div>
    <div class="market-row">
      <span class="market-name">Gasoline</span>
      <span class="market-price" style="color: var(--accent-green)">£${view.market.gasoline_price.toFixed(2)}</span>
    </div>
    <div class="market-row">
      <span class="market-name">Diesel</span>
      <span class="market-price" style="color: var(--accent-cyan)">£${view.market.diesel_price.toFixed(2)}</span>
    </div>
    <div class="market-row" style="border-bottom: none;">
      <span class="market-name" style="font-size: 0.75rem; color: var(--text-muted)">Gasoline Spread</span>
      <span class="market-price" style="font-size: 0.85rem; color: var(--text-secondary)">
        £${(view.market.gasoline_price - view.market.crude_price).toFixed(2)}
      </span>
    </div>
    <div class="market-row" style="border-bottom: none;">
      <span class="market-name" style="font-size: 0.75rem; color: var(--text-muted)">Diesel Spread</span>
      <span class="market-price" style="font-size: 0.85rem; color: var(--text-secondary)">
        £${(view.market.diesel_price - view.market.crude_price).toFixed(2)}
      </span>
    </div>
  `;
}

// ── Products ────────────────────────────────────────────────────────────────
const BLEND_COLORS = [
  '#1a73e8', '#00c853', '#ff9100', '#8b5cf6', '#06b6d4', '#ef4444'
];

function renderProducts(view) {
  const container = document.getElementById('products-content');
  container.innerHTML = view.products.map(p => {
    const totalVol = p.blend.reduce((s, [, v]) => s + v, 0);
    const blendBars = p.blend.map(([name, vol], i) => {
      const pct = totalVol > 0 ? (vol / totalVol * 100) : 0;
      return `<div class="blend-segment" style="width: ${pct}%; background: ${BLEND_COLORS[i % BLEND_COLORS.length]}"></div>`;
    }).join('');
    const legend = p.blend.map(([name, vol], i) =>
      `<span class="blend-legend-item">
        <span class="blend-dot" style="background: ${BLEND_COLORS[i % BLEND_COLORS.length]}"></span>
        ${name} (${formatNum(vol)})
      </span>`
    ).join('');

    return `
      <div class="product-card">
        <div class="product-header">
          <span class="product-name">${p.name}</span>
          <span class="product-volume">${formatNum(p.volume)} bbl/d @ £${p.price.toFixed(0)}</span>
        </div>
        <div class="blend-bar">${blendBars}</div>
        <div class="blend-legend">${legend}</div>
      </div>
    `;
  }).join('');
}

// ── Maintenance Buttons ─────────────────────────────────────────────────────
function renderMaintenance(view) {
  const container = document.getElementById('maintenance-buttons');
  container.innerHTML = view.units.map(u => {
    const disabled = u.maintenance_status !== 'Running' ? 'disabled' : '';
    const healthStr = (u.health * 100).toFixed(0);
    return `
      <div class="action-card">
        <div class="action-info">
          <div class="action-title">${u.name} Turnaround</div>
          <div class="action-detail">Health: ${healthStr}%</div>
        </div>
        <button class="btn action-btn btn-warning" ${disabled}
                onclick="window.scheduleTurnaround('${u.name}')">
          Schedule
        </button>
      </div>
    `;
  }).join('');
}

// ── Capital Projects ────────────────────────────────────────────────────────
function renderProjects(view) {
  const container = document.getElementById('projects-list');

  const active = view.active_projects.map(p => `
    <div class="action-card">
      <div class="action-info">
        <div class="action-title">${p.name}</div>
        <div class="action-detail">${p.unit_name} +${formatNum(p.capacity_gain)} bbl/d — ${p.weeks_remaining}w remaining</div>
      </div>
      <span class="btn action-btn" style="cursor: default; opacity: 0.5">In Progress</span>
    </div>
  `).join('');

  const available = view.available_projects.map(p => `
    <div class="action-card">
      <div class="action-info">
        <div class="action-title">${p.name}</div>
        <div class="action-detail">${p.unit_name} +${formatNum(p.capacity_gain)} bbl/d — ${formatMoney(p.cost)}, ${p.duration_weeks}w</div>
      </div>
      <button class="btn action-btn btn-success"
              onclick="window.approveProject(${p.config_index})">
        Approve
      </button>
    </div>
  `).join('');

  container.innerHTML = active + available || '<div style="color: var(--text-muted); font-size: 0.8rem;">No projects available yet</div>';
}

// ── Events ──────────────────────────────────────────────────────────────────
function renderEvents(view) {
  const container = document.getElementById('events-content');
  // Only append new events
  const newEvents = view.events.slice(lastEventCount);
  lastEventCount = view.events.length;

  for (const evt of newEvents) {
    const el = document.createElement('div');
    const sevClass = evt.severity === 'Critical' ? 'critical' : evt.severity === 'Warning' ? 'warning' : '';
    el.className = `event-item ${sevClass}`;
    el.innerHTML = `
      <span class="event-week">W${evt.week}</span>
      <span class="event-message">${evt.message}</span>
    `;
    container.prepend(el); // newest first
  }

  // Keep max 100 events in DOM
  while (container.children.length > 100) {
    container.removeChild(container.lastChild);
  }
}

// ── Chart ───────────────────────────────────────────────────────────────────
function renderChart(view) {
  const canvas = document.getElementById('chart-history');
  drawHistoryChart(canvas, view.history, view.victory_target);
}

// ── Game Over ───────────────────────────────────────────────────────────────
function showGameOver(status) {
  const overlay = document.getElementById('game-over');
  const title = document.getElementById('game-over-title');
  const msg = document.getElementById('game-over-message');

  overlay.classList.remove('hidden');

  if (status.Won) {
    title.textContent = '🏆 Victory!';
    title.className = 'overlay-title victory';
    msg.textContent = `You reached a £500M valuation in Week ${status.Won.week}. Outstanding work.`;
  } else if (status.Lost) {
    title.textContent = '💀 Game Over';
    title.className = 'overlay-title defeat';
    msg.textContent = `${status.Lost.reason} (Week ${status.Lost.week})`;
  }
}

// ── Global action handlers (called from onclick in rendered HTML) ──────────
window.scheduleTurnaround = (unitName) => {
  pendingActions.push({ ScheduleTurnaround: unitName });
};

window.approveProject = (configIndex) => {
  pendingActions.push({ ApproveProject: configIndex });
};

// ── Launch ──────────────────────────────────────────────────────────────────
init();
