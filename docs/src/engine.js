const SORT_KEY = Object.freeze({
  TTK: 'ttk',
  DPS: 'dps',
  DAMAGE: 'damage',
  DAMAGE_END: 'damageend',
  FIRE_RATE: 'firerate',
  MAGAZINE: 'magazine',
});

const PRIORITY = Object.freeze({ HIGHEST: 'highest', LOWEST: 'lowest', AUTO: 'auto' });

export function normalizeData(raw) {
  if (raw && Number(raw.version) === 1 && Array.isArray(raw.cores)) {
    return normalizeWebData(raw);
  }

  return normalizeLegacyData(raw ?? {});
}

function normalizeWebData(raw) {
  return {
    cores: (raw.cores || []).map((core) => ({
      name: String(core.name || ''),
      category: String(core.category || ''),
      damage: Number(core.damage || 0),
      damageEnd: Number(core.damage_end || 0),
      fireRate: Number(core.fire_rate || 0),
    })),
    magazines: (raw.magazines || []).map((magazine) => ({
      name: String(magazine.name || ''),
      category: String(magazine.category || ''),
      magazineSize: Number(magazine.magazine_size || 0),
      damageMod: Number(magazine.damage_mod || 0),
      fireRateMod: Number(magazine.fire_rate_mod || 0),
    })),
    barrels: normalizeWebParts(raw.barrels || []),
    stocks: normalizeWebParts(raw.stocks || []),
    grips: normalizeWebParts(raw.grips || []),
    penalties: Array.isArray(raw.penalties) ? raw.penalties : [],
    categories: normalizeCategoryMap(raw.categories || {}),
  };
}

function normalizeWebParts(parts) {
  return parts.map((part) => ({
    name: String(part.name || ''),
    category: String(part.category || ''),
    damageMod: Number(part.damage_mod || 0),
    fireRateMod: Number(part.fire_rate_mod || 0),
  }));
}

function normalizeLegacyData(raw) {
  const categories = {};
  for (const group of Object.values(raw.Categories || {})) {
    for (const [name, idx] of Object.entries(group || {})) categories[name] = Number(idx);
  }

  return {
    cores: (raw.Data?.Cores || []).map((core) => {
      const damage = Array.isArray(core.Damage) ? core.Damage : [core.Damage, core.Damage];
      return {
        name: core.Name,
        category: core.Category,
        damage: Number(damage?.[0] || 0),
        damageEnd: Number(damage?.[1] || 0),
        fireRate: Number(core.Fire_Rate || 0),
      };
    }),
    magazines: (raw.Data?.Magazines || []).map((magazine) => ({
      name: magazine.Name,
      category: magazine.Category,
      magazineSize: Number(magazine.Magazine_Size || 0),
      damageMod: Number(magazine.Damage || 0),
      fireRateMod: Number(magazine.Fire_Rate || 0),
    })),
    barrels: mapLegacyPart(raw.Data?.Barrels || []),
    stocks: mapLegacyPart(raw.Data?.Stocks || []),
    grips: mapLegacyPart(raw.Data?.Grips || []),
    penalties: raw.Penalties || [],
    categories,
  };
}

function mapLegacyPart(parts) {
  return parts.map((part) => ({
    name: part.Name,
    category: part.Category,
    damageMod: Number(part.Damage || 0),
    fireRateMod: Number(part.Fire_Rate || 0),
  }));
}

function normalizeCategoryMap(categories) {
  return Object.fromEntries(Object.entries(categories).map(([name, index]) => [name, Number(index)]));
}

export function calculateTop(data, config) {
  return calculate(data, config).results;
}

export function calculate(data, config) {
  const normalizedConfig = normalizeConfig(config);
  const ranking = rankingFor(normalizedConfig);
  const results = [];
  const stats = {
    coresConsidered: 0,
    coresSkippedByCategory: 0,
    combinationsEvaluated: 0,
    combinationsFiltered: 0,
    resultsKept: 0,
  };

  for (const core of data.cores) {
    stats.coresConsidered += 1;

    if (!includeCategory(normalizedConfig.includeCategories, core.category)) {
      stats.coresSkippedByCategory += 1;
      continue;
    }

    const coreIdx = data.categories[core.category];
    if (coreIdx === undefined) continue;

    const magazines = topMagazines(data.magazines, core, coreIdx, normalizedConfig.partPoolPerType, data);
    const barrels = topParts(data.barrels, core, coreIdx, normalizedConfig.partPoolPerType, data);
    const stocks = topParts(data.stocks, core, coreIdx, normalizedConfig.partPoolPerType, data);
    const grips = topParts(data.grips, core, coreIdx, normalizedConfig.partPoolPerType, data);

    for (const magazine of magazines) {
      for (const barrel of barrels) {
        const magazineBarrelDamage = magazine.damageFactor * barrel.damageFactor;
        const magazineBarrelFireRate = magazine.fireRateFactor * barrel.fireRateFactor;

        for (const stock of stocks) {
          const baseDamageFactor = magazineBarrelDamage * stock.damageFactor;
          const baseFireRateFactor = magazineBarrelFireRate * stock.fireRateFactor;

          for (const grip of grips) {
            stats.combinationsEvaluated += 1;

            const metrics = evaluateMetrics(
              normalizedConfig,
              core,
              baseDamageFactor * grip.damageFactor,
              baseFireRateFactor * grip.fireRateFactor,
              magazine.item.magazineSize,
            );

            if (!metrics) continue;
            if (!passesFilters(normalizedConfig, metrics)) {
              stats.combinationsFiltered += 1;
              continue;
            }

            if (!canEnterTop(results, metrics, normalizedConfig.topN, ranking)) continue;

            insertTopResult(results, {
              core: core.name,
              magazine: magazine.item.name,
              barrel: barrel.item.name,
              stock: stock.item.name,
              grip: grip.item.name,
              ...metrics,
            }, normalizedConfig.topN, ranking);
          }
        }
      }
    }
  }

  stats.resultsKept = results.length;
  return { results, stats };
}

function normalizeConfig(config = {}) {
  return {
    topN: clampInteger(config.topN, 1, 100, 10),
    playerMaxHealth: positiveNumber(config.playerMaxHealth, 100),
    sortKey: Object.values(SORT_KEY).includes(config.sortKey) ? config.sortKey : SORT_KEY.TTK,
    priority: Object.values(PRIORITY).includes(config.priority) ? config.priority : PRIORITY.AUTO,
    includeCategories: Array.isArray(config.includeCategories) ? config.includeCategories.filter(Boolean) : [],
    partPoolPerType: clampInteger(config.partPoolPerType, 1, 50, 20),
    damageRange: normalizeRange(config.damageRange),
    damageEndRange: normalizeRange(config.damageEndRange),
    ttkSecondsRange: normalizeRange(config.ttkSecondsRange),
    dpsRange: normalizeRange(config.dpsRange),
  };
}

function normalizeRange(range) {
  const min = optionalFiniteNumber(range?.min);
  const max = optionalFiniteNumber(range?.max);
  return { min, max };
}

function optionalFiniteNumber(value) {
  if (value === '' || value === null || value === undefined) return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function positiveNumber(value, fallback) {
  const number = Number(value);
  return Number.isFinite(number) && number > 0 ? number : fallback;
}

function clampInteger(value, min, max, fallback) {
  const number = Number.parseInt(value, 10);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(max, Math.max(min, number));
}

function topParts(parts, core, coreIdx, limit, data) {
  return parts
    .map((part) => evaluatePart(part, core, coreIdx, data))
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

function topMagazines(magazines, core, coreIdx, limit, data) {
  return magazines
    .map((magazine) => {
      const evaluated = evaluatePart(magazine, core, coreIdx, data);
      return { ...evaluated, score: evaluated.score + magazine.magazineSize * 0.05 };
    })
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

function evaluatePart(part, core, coreIdx, data) {
  const penalty = core.name === part.name ? 0 : penaltyFor(data, coreIdx, part.category);
  const adjustedDamage = Number(part.damageMod || 0) * penalty;
  const adjustedFireRate = Number(part.fireRateMod || 0) * penalty;

  return {
    item: part,
    damageFactor: 1 + adjustedDamage / 100,
    fireRateFactor: 1 + adjustedFireRate / 100,
    score: adjustedDamage + adjustedFireRate * 0.6,
  };
}

function penaltyFor(data, coreIdx, category) {
  const partIdx = data.categories[category];
  if (partIdx === undefined) return 1;
  return Number(data.penalties?.[coreIdx]?.[partIdx] ?? 1);
}

function evaluateMetrics(config, core, damageFactor, fireRateFactor, magazineSize) {
  const damage = core.damage * damageFactor;
  const fireRate = core.fireRate * fireRateFactor;
  if (damage <= 0 || fireRate <= 0) return null;

  const shots = Math.ceil(config.playerMaxHealth / damage);
  const ttkSeconds = ((shots - 1) / fireRate) * 60;

  return {
    damage,
    damageEnd: core.damageEnd * damageFactor,
    fireRate,
    magazineSize,
    ttkSeconds,
    dps: (damage * fireRate) / 60,
  };
}

function passesFilters(config, metrics) {
  return inRange(config.damageRange, metrics.damage)
    && inRange(config.damageEndRange, metrics.damageEnd)
    && inRange(config.ttkSecondsRange, metrics.ttkSeconds)
    && inRange(config.dpsRange, metrics.dps);
}

function inRange(range, value) {
  if (range.min !== null && value < range.min) return false;
  if (range.max !== null && value > range.max) return false;
  return true;
}

function includeCategory(allowed, category) {
  return !allowed.length || allowed.some((candidate) => candidate.toLowerCase() === category.toLowerCase());
}

function rankingFor(config) {
  const priority = config.priority === PRIORITY.AUTO
    ? (config.sortKey === SORT_KEY.TTK ? PRIORITY.LOWEST : PRIORITY.HIGHEST)
    : config.priority;

  return { key: config.sortKey, priority };
}

function canEnterTop(results, metrics, topN, ranking) {
  if (results.length < topN) return true;
  return isBetter(metric(metrics, ranking.key), metric(results[results.length - 1], ranking.key), ranking.priority);
}

function insertTopResult(results, candidate, topN, ranking) {
  if (topN <= 0) return;

  const candidateMetric = metric(candidate, ranking.key);
  let insertAt = results.findIndex((result) => isBetter(candidateMetric, metric(result, ranking.key), ranking.priority));
  if (insertAt === -1) insertAt = results.length;
  results.splice(insertAt, 0, candidate);
  if (results.length > topN) results.pop();
}

function isBetter(left, right, priority) {
  return priority === PRIORITY.HIGHEST ? left > right : left < right;
}

function metric(result, key) {
  switch (key) {
    case SORT_KEY.DPS: return result.dps;
    case SORT_KEY.DAMAGE: return result.damage;
    case SORT_KEY.DAMAGE_END: return result.damageEnd;
    case SORT_KEY.FIRE_RATE: return result.fireRate;
    case SORT_KEY.MAGAZINE: return result.magazineSize;
    case SORT_KEY.TTK:
    default: return result.ttkSeconds;
  }
}

export function formatResults(results) {
  if (!results.length) return 'No results found.';
  return results.map((result, index) => [
    `#${index + 1}`,
    `Core: ${result.core}`,
    `Magazine: ${result.magazine}`,
    `Barrel: ${result.barrel}`,
    `Stock: ${result.stock}`,
    `Grip: ${result.grip}`,
    `Damage: ${result.damage.toFixed(3)}`,
    `Damage End: ${result.damageEnd.toFixed(3)}`,
    `Fire Rate: ${result.fireRate.toFixed(3)}`,
    `Magazine: ${result.magazineSize.toFixed(0)}`,
    `TTK: ${result.ttkSeconds.toFixed(3)}s`,
    `DPS: ${result.dps.toFixed(3)}`,
  ].join('\n')).join('\n\n');
}

export { SORT_KEY, PRIORITY };
