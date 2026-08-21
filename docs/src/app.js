const DATA_URL = './data.json';
const STORAGE_KEY = 'wggcalc:web-config:v1';
const numberFormatter = new Intl.NumberFormat('en-US');

const elements = {
  form: document.querySelector('#calculatorForm'),
  categoryList: document.querySelector('#categoryList'),
  resetButton: document.querySelector('#resetButton'),
  resultsBody: document.querySelector('#resultsBody'),
  emptyState: document.querySelector('#emptyState'),
  tableWrap: document.querySelector('#tableWrap'),
  statusText: document.querySelector('#statusText'),
  statusDot: document.querySelector('#statusDot'),
  resultCount: document.querySelector('#resultCount'),
  elapsedTime: document.querySelector('#elapsedTime'),
  combinations: document.querySelector('#combinations'),
  dataSummary: document.querySelector('#dataSummary'),
  hero: document.querySelector('#heroResult'),
  heroTitle: document.querySelector('#heroTitle'),
  heroBuild: document.querySelector('#heroBuild'),
  heroStats: document.querySelector('#heroStats'),
};

let worker;
let debounceTimer;
let requestId = 0;
let latestRequestedId = 0;
let ready = false;

restoreSettings();
startWorker();

for (const eventName of ['input', 'change']) {
  elements.form.addEventListener(eventName, (event) => {
    if (event.target.matches('input, select')) {
      saveSettings();
      scheduleCalculation();
    }
  });
}

elements.resetButton.addEventListener('click', () => {
  elements.form.reset();
  document.querySelector('#topN').value = '10';
  document.querySelector('#maxHealth').value = '100';
  document.querySelector('#partPool').value = '20';
  document.querySelector('#sortKey').value = 'ttk';
  document.querySelector('#priority').value = 'auto';
  for (const checkbox of elements.categoryList.querySelectorAll('input[type="checkbox"]')) checkbox.checked = false;
  localStorage.removeItem(STORAGE_KEY);
  scheduleCalculation(0);
});

elements.resultsBody.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-copy-build]');
  if (!button) return;

  try {
    await navigator.clipboard.writeText(button.dataset.copyBuild);
    const previous = button.textContent;
    button.textContent = 'Copied';
    setTimeout(() => { button.textContent = previous; }, 1200);
  } catch {
    setStatus('Could not copy build', 'error');
  }
});

function startWorker() {
  ready = false;
  setStatus('Loading calculator data…', 'loading');
  worker = new Worker('./src/worker.js', { type: 'module' });
  worker.addEventListener('message', handleWorkerMessage);
  worker.addEventListener('error', () => setStatus('Calculator worker failed to start', 'error'));
  worker.postMessage({ type: 'init', dataUrl: DATA_URL });
}

function handleWorkerMessage(event) {
  const message = event.data;

  if (message.type === 'ready') {
    ready = true;
    renderCategories(message.categories);
    elements.dataSummary.textContent = `${message.counts.cores} cores · ${message.counts.magazines} magazines · ${message.counts.barrels + message.counts.stocks + message.counts.grips} parts`;
    setStatus('Live calculator ready', 'ready');
    scheduleCalculation(0);
    return;
  }

  if (message.type === 'error') {
    if (message.requestId && message.requestId !== latestRequestedId) return;
    setStatus(message.message, 'error');
    renderEmpty('Calculation unavailable', message.message);
    return;
  }

  if (message.type === 'result') {
    if (message.requestId !== latestRequestedId) return;
    renderResults(message.results, message.stats, message.elapsedMs);
    setStatus('Updated live', 'ready');
  }
}

function scheduleCalculation(delay = 160) {
  clearTimeout(debounceTimer);
  if (!ready) return;
  debounceTimer = setTimeout(runCalculation, delay);
}

function runCalculation() {
  const validation = validateInputs();
  if (validation) {
    setStatus(validation, 'error');
    return;
  }

  requestId += 1;
  latestRequestedId = requestId;
  setStatus('Calculating…', 'loading');
  worker.postMessage({ type: 'calculate', requestId, config: readConfig() });
}

function readConfig() {
  return {
    topN: numericValue('#topN', 10),
    playerMaxHealth: numericValue('#maxHealth', 100),
    sortKey: document.querySelector('#sortKey').value,
    priority: document.querySelector('#priority').value,
    includeCategories: [...elements.categoryList.querySelectorAll('input:checked')].map((input) => input.value),
    partPoolPerType: numericValue('#partPool', 20),
    damageRange: readRange('damage'),
    damageEndRange: readRange('damageEnd'),
    ttkSecondsRange: readRange('ttk'),
    dpsRange: readRange('dps'),
  };
}

function readRange(prefix) {
  return {
    min: optionalNumericValue(`#${prefix}Min`),
    max: optionalNumericValue(`#${prefix}Max`),
  };
}

function validateInputs() {
  const health = numericValue('#maxHealth', NaN);
  if (!Number.isFinite(health) || health <= 0) return 'Player health must be greater than zero.';

  const topN = numericValue('#topN', NaN);
  if (!Number.isInteger(topN) || topN < 1 || topN > 100) return 'Top results must be between 1 and 100.';

  const partPool = numericValue('#partPool', NaN);
  if (!Number.isInteger(partPool) || partPool < 1 || partPool > 50) return 'Part pool must be between 1 and 50.';

  for (const [label, prefix] of [['Damage', 'damage'], ['Damage end', 'damageEnd'], ['TTK', 'ttk'], ['DPS', 'dps']]) {
    const min = optionalNumericValue(`#${prefix}Min`);
    const max = optionalNumericValue(`#${prefix}Max`);
    if (min !== null && max !== null && min > max) return `${label} minimum cannot exceed its maximum.`;
  }

  return null;
}

function renderCategories(categories) {
  const selected = new Set(readSavedSettings()?.includeCategories || []);
  elements.categoryList.replaceChildren(...categories.map((category) => {
    const label = document.createElement('label');
    label.className = 'category-chip';

    const input = document.createElement('input');
    input.type = 'checkbox';
    input.value = category;
    input.checked = selected.has(category);

    const span = document.createElement('span');
    span.textContent = category;
    label.append(input, span);
    return label;
  }));
}

function renderResults(results, stats, elapsedMs) {
  elements.resultCount.textContent = numberFormatter.format(results.length);
  elements.elapsedTime.textContent = `${elapsedMs.toFixed(elapsedMs < 10 ? 2 : 1)} ms`;
  elements.combinations.textContent = numberFormatter.format(stats.combinationsEvaluated);

  if (!results.length) {
    elements.hero.hidden = true;
    elements.tableWrap.hidden = true;
    renderEmpty('No builds match these filters', 'Loosen one or more ranges or include additional categories.');
    return;
  }

  elements.emptyState.hidden = true;
  elements.tableWrap.hidden = false;
  renderHero(results[0]);
  elements.resultsBody.replaceChildren(...results.map(renderResultRow));
}

function renderHero(result) {
  elements.hero.hidden = false;
  elements.heroTitle.textContent = result.core;
  elements.heroBuild.textContent = `${result.magazine} · ${result.barrel} · ${result.stock} · ${result.grip}`;
  elements.heroStats.replaceChildren(
    statPill('TTK', `${formatMetric(result.ttkSeconds)}s`),
    statPill('DPS', formatMetric(result.dps)),
    statPill('Damage', formatMetric(result.damage)),
    statPill('RPM', formatMetric(result.fireRate)),
  );
}

function statPill(label, value) {
  const item = document.createElement('div');
  item.className = 'hero-stat';
  const strong = document.createElement('strong');
  strong.textContent = value;
  const span = document.createElement('span');
  span.textContent = label;
  item.append(strong, span);
  return item;
}

function renderResultRow(result, index) {
  const row = document.createElement('tr');
  const build = `${result.core} / ${result.magazine} / ${result.barrel} / ${result.stock} / ${result.grip}`;
  row.innerHTML = `
    <td class="rank-cell">#${index + 1}</td>
    <td><strong>${escapeHtml(result.core)}</strong><span class="part-stack">${escapeHtml(result.magazine)} · ${escapeHtml(result.barrel)} · ${escapeHtml(result.stock)} · ${escapeHtml(result.grip)}</span></td>
    <td class="number-cell">${formatMetric(result.damage)}</td>
    <td class="number-cell">${formatMetric(result.damageEnd)}</td>
    <td class="number-cell">${formatMetric(result.fireRate)}</td>
    <td class="number-cell">${formatMetric(result.magazineSize, 0)}</td>
    <td class="number-cell metric-primary">${formatMetric(result.ttkSeconds)}s</td>
    <td class="number-cell">${formatMetric(result.dps)}</td>
    <td><button class="copy-button" type="button">Copy</button></td>
  `;
  row.querySelector('.copy-button').dataset.copyBuild = build;
  return row;
}

function renderEmpty(title, description) {
  elements.tableWrap.hidden = true;
  elements.emptyState.hidden = false;
  elements.emptyState.querySelector('strong').textContent = title;
  elements.emptyState.querySelector('span').textContent = description;
}

function setStatus(text, state) {
  elements.statusText.textContent = text;
  elements.statusDot.dataset.state = state;
}

function formatMetric(value, decimals = 2) {
  return Number(value).toLocaleString('en-US', { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
}

function numericValue(selector, fallback) {
  const value = Number(document.querySelector(selector).value);
  return Number.isFinite(value) ? value : fallback;
}

function optionalNumericValue(selector) {
  const raw = document.querySelector(selector).value.trim();
  if (!raw) return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

function saveSettings() {
  try {
    const config = readConfig();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    // Storage is optional; calculation should continue when it is unavailable.
  }
}

function readSavedSettings() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
  } catch {
    return null;
  }
}

function restoreSettings() {
  const saved = readSavedSettings();
  if (!saved) return;

  setInputValue('#topN', saved.topN);
  setInputValue('#maxHealth', saved.playerMaxHealth);
  setInputValue('#partPool', saved.partPoolPerType);
  setInputValue('#sortKey', saved.sortKey);
  setInputValue('#priority', saved.priority);
  setRangeValues('damage', saved.damageRange);
  setRangeValues('damageEnd', saved.damageEndRange);
  setRangeValues('ttk', saved.ttkSecondsRange);
  setRangeValues('dps', saved.dpsRange);
}

function setRangeValues(prefix, range) {
  if (!range) return;
  setInputValue(`#${prefix}Min`, range.min ?? '');
  setInputValue(`#${prefix}Max`, range.max ?? '');
}

function setInputValue(selector, value) {
  const element = document.querySelector(selector);
  if (element && value !== undefined && value !== null) element.value = String(value);
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}
