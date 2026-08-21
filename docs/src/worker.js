import { calculate, normalizeData } from './engine.js';

let data = null;

self.addEventListener('message', async (event) => {
  const message = event.data;

  if (message.type === 'init') {
    try {
      const siteRoot = new URL('../', self.location.href);
      const datasetUrl = new URL(message.dataUrl, siteRoot);
      const response = await fetch(datasetUrl, { cache: 'no-cache' });
      if (!response.ok) throw new Error(`Dataset request failed (${response.status}) for ${datasetUrl.pathname}`);
      data = normalizeData(await response.json());

      self.postMessage({
        type: 'ready',
        categories: Object.keys(data.categories).sort((a, b) => a.localeCompare(b)),
        counts: {
          cores: data.cores.length,
          magazines: data.magazines.length,
          barrels: data.barrels.length,
          stocks: data.stocks.length,
          grips: data.grips.length,
        },
      });
    } catch (error) {
      self.postMessage({ type: 'error', message: errorMessage(error) });
    }
    return;
  }

  if (message.type === 'calculate') {
    if (!data) {
      self.postMessage({ type: 'error', message: 'Calculator data is not loaded yet.' });
      return;
    }

    const startedAt = performance.now();
    try {
      const calculation = calculate(data, message.config);
      self.postMessage({
        type: 'result',
        requestId: message.requestId,
        elapsedMs: performance.now() - startedAt,
        ...calculation,
      });
    } catch (error) {
      self.postMessage({
        type: 'error',
        requestId: message.requestId,
        message: errorMessage(error),
      });
    }
  }
});

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}
