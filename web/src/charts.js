import { Chart, registerables } from 'chart.js';

// Register all Chart.js components (controllers, elements, scales, etc.)
Chart.register(...registerables);

/**
 * Draw the performance history chart using Chart.js.
 * Reuses or recreates the chart instance bound to the canvas to prevent layering.
 * 
 * @param {HTMLCanvasElement} canvas 
 * @param {Array} history 
 * @param {number} victoryTarget 
 */
export function drawHistoryChart(canvas, history, victoryTarget) {
  const ctx = canvas.getContext('2d');
  
  if (history.length < 2) {
    // Canvas loading state
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = '#94a3b8';
    ctx.font = '13px Outfit, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText('Accumulating telemetry data...', canvas.width / 2, canvas.height / 2);
    return;
  }

  const weeks = history.map(h => `W${h.week}`);
  const valuations = history.map(h => h.valuation);
  const margins = history.map(h => h.margin);
  const debts = history.map(h => h.debt);
  const targetLine = history.map(() => victoryTarget);

  // Destroy previous instance to avoid conflicts
  if (canvas._chart) {
    canvas._chart.destroy();
  }

  // Create new Chart.js instance
  canvas._chart = new Chart(ctx, {
    type: 'line',
    data: {
      labels: weeks,
      datasets: [
        {
          label: 'Enterprise Value',
          data: valuations,
          borderColor: '#00e5ff',
          backgroundColor: 'rgba(0, 229, 255, 0.05)',
          borderWidth: 2.5,
          tension: 0.2,
          yAxisID: 'yValuation',
          fill: true,
        },
        {
          label: 'Weekly Margin (EBITDA)',
          data: margins,
          borderColor: '#00e676',
          backgroundColor: 'transparent',
          borderWidth: 2.5,
          tension: 0.1,
          yAxisID: 'yMargin',
        },
        {
          label: 'Debt Principal',
          data: debts,
          borderColor: '#ffb300',
          backgroundColor: 'transparent',
          borderWidth: 1.5,
          borderDash: [4, 4],
          tension: 0,
          yAxisID: 'yValuation',
        },
        {
          label: 'Target (£500M)',
          data: targetLine,
          borderColor: 'rgba(255, 23, 68, 0.4)',
          borderWidth: 1.5,
          borderDash: [6, 6],
          pointRadius: 0,
          tension: 0,
          yAxisID: 'yValuation',
          fill: false,
        }
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: {
        mode: 'index',
        intersect: false,
      },
      plugins: {
        legend: {
          position: 'top',
          labels: {
            color: '#94a3b8',
            font: {
              family: 'Outfit, sans-serif',
              size: 11
            },
            padding: 15
          }
        },
        tooltip: {
          backgroundColor: '#0f1424',
          titleColor: '#38bdf8',
          titleFont: { family: 'Outfit', size: 12, weight: 'bold' },
          bodyColor: '#f1f5f9',
          bodyFont: { family: 'JetBrains Mono', size: 11 },
          borderColor: '#1e2640',
          borderWidth: 1,
          padding: 10,
          callbacks: {
            label: function(context) {
              let label = context.dataset.label || '';
              if (label) {
                label += ': ';
              }
              if (context.parsed.y !== null) {
                label += formatMoney(context.parsed.y);
              }
              return label;
            }
          }
        }
      },
      scales: {
        x: {
          grid: {
            color: 'rgba(30, 38, 64, 0.3)',
            drawBorder: false
          },
          ticks: {
            color: '#475569',
            font: { family: 'JetBrains Mono', size: 9 }
          }
        },
        yValuation: {
          type: 'linear',
          position: 'left',
          grid: {
            color: 'rgba(30, 38, 64, 0.3)',
            drawBorder: false
          },
          ticks: {
            color: '#00e5ff',
            font: { family: 'JetBrains Mono', size: 9 },
            callback: function(value) {
              return formatCompact(value);
            }
          },
          title: {
            display: true,
            text: 'Valuation / Debt (£)',
            color: '#00e5ff',
            font: { family: 'Outfit', size: 10 }
          }
        },
        yMargin: {
          type: 'linear',
          position: 'right',
          grid: {
            drawOnChartArea: false, // Only draw grid lines for the left axis
            drawBorder: false
          },
          ticks: {
            color: '#00e676',
            font: { family: 'JetBrains Mono', size: 9 },
            callback: function(value) {
              return formatCompact(value);
            }
          },
          title: {
            display: true,
            text: 'EBITDA Margin (£/wk)',
            color: '#00e676',
            font: { family: 'Outfit', size: 10 }
          }
        }
      }
    }
  });
}

function formatMoney(n) {
  const abs = Math.abs(n);
  const sign = n < 0 ? '-' : '';
  if (abs >= 1e9) return sign + '£' + (abs / 1e9).toFixed(2) + 'B';
  if (abs >= 1e6) return sign + '£' + (abs / 1e6).toFixed(1) + 'M';
  if (abs >= 1e3) return sign + '£' + (abs / 1e3).toFixed(0) + 'K';
  return sign + '£' + abs.toFixed(0);
}

function formatCompact(n) {
  const abs = Math.abs(n);
  const sign = n < 0 ? '-' : '';
  if (abs >= 1e9) return sign + (abs / 1e9).toFixed(1) + 'B';
  if (abs >= 1e6) return sign + (abs / 1e6).toFixed(1) + 'M';
  if (abs >= 1e3) return sign + (abs / 1e3).toFixed(0) + 'K';
  return sign + abs.toFixed(0);
}
