/**
 * charts.js — Lightweight canvas-based sparklines for the performance history panel.
 * No external dependencies. Draws margin and valuation over time.
 */

const CHART_COLORS = {
  margin: { stroke: '#00c853', fill: 'rgba(0, 200, 83, 0.08)' },
  valuation: { stroke: '#06b6d4', fill: 'rgba(6, 182, 212, 0.06)' },
  grid: '#1e2536',
  axis: '#2a3144',
  text: '#5a6178',
  background: '#1a1f2e',
};

/**
 * Draw the performance history chart on the given canvas.
 * @param {HTMLCanvasElement} canvas
 * @param {Array} history — array of WeekSnapshot objects
 * @param {number} victoryTarget — valuation target in £
 */
export function drawHistoryChart(canvas, history, victoryTarget) {
  const ctx = canvas.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();

  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.scale(dpr, dpr);

  const w = rect.width;
  const h = rect.height;
  const pad = { top: 20, right: 70, bottom: 30, left: 65 };
  const plotW = w - pad.left - pad.right;
  const plotH = h - pad.top - pad.bottom;

  // Clear
  ctx.fillStyle = CHART_COLORS.background;
  ctx.fillRect(0, 0, w, h);

  if (history.length < 2) {
    ctx.fillStyle = CHART_COLORS.text;
    ctx.font = '12px Inter, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('Accumulating data…', w / 2, h / 2);
    return;
  }

  // Data ranges
  const margins = history.map(s => s.margin);
  const valuations = history.map(s => s.valuation);
  const weeks = history.map(s => s.week);

  const maxMargin = Math.max(...margins) * 1.15;
  const minMargin = Math.min(0, Math.min(...margins) * 1.1);
  const maxVal = Math.max(victoryTarget, ...valuations) * 1.1;

  // Helper: map value to pixel
  const xPx = (i) => pad.left + (i / (history.length - 1)) * plotW;
  const yMargin = (v) => pad.top + plotH - ((v - minMargin) / (maxMargin - minMargin)) * plotH;
  const yVal = (v) => pad.top + plotH - (v / maxVal) * plotH;

  // Grid lines
  ctx.strokeStyle = CHART_COLORS.grid;
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = pad.top + (plotH / 4) * i;
    ctx.beginPath();
    ctx.moveTo(pad.left, y);
    ctx.lineTo(pad.left + plotW, y);
    ctx.stroke();
  }

  // Victory target line (dashed)
  ctx.setLineDash([6, 4]);
  ctx.strokeStyle = 'rgba(6, 182, 212, 0.3)';
  ctx.lineWidth = 1;
  const targetY = yVal(victoryTarget);
  if (targetY > pad.top && targetY < pad.top + plotH) {
    ctx.beginPath();
    ctx.moveTo(pad.left, targetY);
    ctx.lineTo(pad.left + plotW, targetY);
    ctx.stroke();
    ctx.fillStyle = 'rgba(6, 182, 212, 0.5)';
    ctx.font = '10px Inter, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('£500M Target', pad.left + plotW + 4, targetY + 4);
  }
  ctx.setLineDash([]);

  // --- Margin area fill ---
  ctx.beginPath();
  ctx.moveTo(xPx(0), yMargin(0));
  for (let i = 0; i < history.length; i++) {
    ctx.lineTo(xPx(i), yMargin(margins[i]));
  }
  ctx.lineTo(xPx(history.length - 1), yMargin(0));
  ctx.closePath();
  ctx.fillStyle = CHART_COLORS.margin.fill;
  ctx.fill();

  // --- Margin line ---
  ctx.beginPath();
  for (let i = 0; i < history.length; i++) {
    if (i === 0) ctx.moveTo(xPx(i), yMargin(margins[i]));
    else ctx.lineTo(xPx(i), yMargin(margins[i]));
  }
  ctx.strokeStyle = CHART_COLORS.margin.stroke;
  ctx.lineWidth = 2;
  ctx.stroke();

  // --- Valuation line ---
  ctx.beginPath();
  for (let i = 0; i < history.length; i++) {
    if (i === 0) ctx.moveTo(xPx(i), yVal(valuations[i]));
    else ctx.lineTo(xPx(i), yVal(valuations[i]));
  }
  ctx.strokeStyle = CHART_COLORS.valuation.stroke;
  ctx.lineWidth = 2;
  ctx.stroke();

  // --- Axes ---
  ctx.strokeStyle = CHART_COLORS.axis;
  ctx.lineWidth = 1;
  // Left axis (margin)
  ctx.beginPath();
  ctx.moveTo(pad.left, pad.top);
  ctx.lineTo(pad.left, pad.top + plotH);
  ctx.lineTo(pad.left + plotW, pad.top + plotH);
  ctx.stroke();

  // Axis labels — left (margin)
  ctx.fillStyle = CHART_COLORS.margin.stroke;
  ctx.font = '10px JetBrains Mono, monospace';
  ctx.textAlign = 'right';
  for (let i = 0; i <= 4; i++) {
    const val = minMargin + ((maxMargin - minMargin) / 4) * (4 - i);
    const y = pad.top + (plotH / 4) * i;
    ctx.fillText(formatCompact(val), pad.left - 6, y + 4);
  }

  // Axis labels — right (valuation)
  ctx.fillStyle = CHART_COLORS.valuation.stroke;
  ctx.textAlign = 'left';
  for (let i = 0; i <= 4; i++) {
    const val = (maxVal / 4) * (4 - i);
    const y = pad.top + (plotH / 4) * i;
    ctx.fillText(formatCompact(val), pad.left + plotW + 6, y + 4);
  }

  // X-axis labels (week numbers)
  ctx.fillStyle = CHART_COLORS.text;
  ctx.textAlign = 'center';
  const step = Math.max(1, Math.floor(history.length / 8));
  for (let i = 0; i < history.length; i += step) {
    ctx.fillText(`W${weeks[i]}`, xPx(i), pad.top + plotH + 16);
  }

  // Legend
  ctx.font = '10px Inter, sans-serif';
  const legendY = 10;
  // Margin
  ctx.fillStyle = CHART_COLORS.margin.stroke;
  ctx.fillRect(pad.left, legendY, 12, 3);
  ctx.fillText('Margin (£/wk)', pad.left + 16, legendY + 4);
  // Valuation
  ctx.fillStyle = CHART_COLORS.valuation.stroke;
  ctx.fillRect(pad.left + 120, legendY, 12, 3);
  ctx.fillText('Valuation (£)', pad.left + 136, legendY + 4);
}

/** Format a number in compact form (e.g. 1.2M, 450K). */
function formatCompact(n) {
  const abs = Math.abs(n);
  const sign = n < 0 ? '-' : '';
  if (abs >= 1e9) return sign + (abs / 1e9).toFixed(1) + 'B';
  if (abs >= 1e6) return sign + (abs / 1e6).toFixed(1) + 'M';
  if (abs >= 1e3) return sign + (abs / 1e3).toFixed(0) + 'K';
  return sign + abs.toFixed(0);
}
