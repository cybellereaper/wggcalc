import test from 'node:test';
import assert from 'node:assert/strict';
import { calculate, calculateTop, formatResults, normalizeData } from '../src/engine.js';

const rawData = {
  Categories: { Primary: { AR: 0 }, Secondary: {} },
  Penalties: [[1]],
  Data: {
    Cores: [{ Name: 'Core-1', Category: 'AR', Damage: [50, 40], Fire_Rate: 120 }],
    Magazines: [{ Name: 'Mag-1', Category: 'AR', Magazine_Size: 20, Damage: 0, Fire_Rate: 0 }],
    Barrels: [{ Name: 'Barrel-1', Category: 'AR', Damage: 0, Fire_Rate: 0 }],
    Stocks: [{ Name: 'Stock-1', Category: 'AR', Damage: 0, Fire_Rate: 0 }],
    Grips: [{ Name: 'Grip-1', Category: 'AR', Damage: 0, Fire_Rate: 0 }],
  },
};

const baseConfig = {
  topN: 10,
  playerMaxHealth: 100,
  sortKey: 'ttk',
  priority: 'auto',
  includeCategories: [],
  partPoolPerType: 20,
  damageRange: {},
  damageEndRange: {},
  ttkSecondsRange: {},
  dpsRange: {},
};

test('normalizes the Rust web export schema', () => {
  const data = normalizeData({
    version: 1,
    cores: [{ name: 'Core', category: 'AR', damage: 42, damage_end: 30, fire_rate: 600 }],
    magazines: [{ name: 'Mag', category: 'AR', magazine_size: 30, damage_mod: 1, fire_rate_mod: 2 }],
    barrels: [], stocks: [], grips: [], penalties: [[1]], categories: { AR: 0 },
  });

  assert.deepEqual(data.cores[0], { name: 'Core', category: 'AR', damage: 42, damageEnd: 30, fireRate: 600 });
  assert.equal(data.magazines[0].magazineSize, 30);
});

test('calculateTop returns deterministic single result fixture', () => {
  const results = calculateTop(normalizeData(rawData), baseConfig);
  assert.equal(results.length, 1);
  assert.equal(results[0].ttkSeconds, 0.5);
  assert.equal(results[0].dps, 100);
});

test('calculate reports search statistics', () => {
  const { results, stats } = calculate(normalizeData(rawData), baseConfig);
  assert.equal(results.length, 1);
  assert.equal(stats.coresConsidered, 1);
  assert.equal(stats.combinationsEvaluated, 1);
  assert.equal(stats.resultsKept, 1);
});

test('magazine preselection matches Rust size tie-break scoring', () => {
  const fixture = structuredClone(rawData);
  fixture.Data.Magazines = [
    { Name: 'Small', Category: 'AR', Magazine_Size: 10, Damage: 0, Fire_Rate: 0 },
    { Name: 'Large', Category: 'AR', Magazine_Size: 50, Damage: 0, Fire_Rate: 0 },
  ];

  const results = calculateTop(normalizeData(fixture), { ...baseConfig, partPoolPerType: 1 });
  assert.equal(results[0].magazine, 'Large');
});

test('filters can remove all results', () => {
  const results = calculateTop(normalizeData(rawData), {
    ...baseConfig,
    damageRange: { min: 9999 },
  });
  assert.equal(results.length, 0);
});

test('formatResults renders build and magazine size', () => {
  const text = formatResults([{ core: 'C', magazine: 'M', barrel: 'B', stock: 'S', grip: 'G', damage: 1, damageEnd: 1, fireRate: 1, magazineSize: 30, ttkSeconds: 1, dps: 1 }]);
  assert.match(text, /#1/);
  assert.match(text, /Core: C/);
  assert.match(text, /Magazine: 30/);
});
