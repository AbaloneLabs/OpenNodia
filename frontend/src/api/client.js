let sessionActive = false;

export function getToken() {
  return sessionActive ? 'cookie-session' : null;
}

export function setToken(token) {
  sessionActive = Boolean(token);
}

function csrfHeaders(method = 'GET') {
  const normalized = method.toUpperCase();
  if (normalized === 'GET' || normalized === 'HEAD' || normalized === 'OPTIONS') {
    return {};
  }
  return { 'X-OpenNodia-CSRF': '1' };
}

export async function request(path, options = {}) {
  const method = options.method || 'GET';
  const headers = {
    'Content-Type': 'application/json',
    ...csrfHeaders(method),
    ...options.headers,
  };

  const resp = await fetch(path, { ...options, method, headers, credentials: 'same-origin' });

  if (resp.status === 401) {
    setToken(null);
    throw new Error('Session expired. Please log in again.');
  }

  const text = await resp.text();
  const data = text ? JSON.parse(text) : null;

  if (!resp.ok) {
    throw new Error(data?.error || `Request failed (${resp.status})`);
  }

  return data;
}

export async function requestText(path, options = {}) {
  const method = options.method || 'GET';
  const headers = {
    ...csrfHeaders(method),
    ...options.headers,
  };

  const resp = await fetch(path, { ...options, method, headers, credentials: 'same-origin' });

  if (resp.status === 401) {
    setToken(null);
    throw new Error('Session expired. Please log in again.');
  }

  const text = await resp.text();
  if (!resp.ok) {
    let message = `Request failed (${resp.status})`;
    try {
      message = JSON.parse(text)?.error || message;
    } catch {
      if (text) message = text;
    }
    throw new Error(message);
  }
  return text;
}

export function transactionQuery(params = {}) {
  const qs = new URLSearchParams();
  qs.set('limit', String(params.limit || 20));
  if (params.offset) qs.set('offset', String(params.offset));
  if (params.minRound) qs.set('min_round', String(params.minRound));
  if (params.maxRound) qs.set('max_round', String(params.maxRound));
  if (params.fromTime) qs.set('from_time', String(params.fromTime));
  if (params.toTime) qs.set('to_time', String(params.toTime));
  if (params.txType && params.txType !== 'all') qs.set('tx_type', params.txType);
  if (params.assetId !== '' && params.assetId != null && params.assetId !== 'all') {
    qs.set('asset_id', String(params.assetId));
  }
  return qs.toString();
}

export function portfolioQuery(range = '1m') {
  const qs = new URLSearchParams();
  qs.set('range', range || '1m');
  return qs.toString();
}

export function analyticsQuery(params = {}) {
  const qs = new URLSearchParams();
  if (params.limit) qs.set('limit', String(params.limit));
  if (params.minRound) qs.set('min_round', String(params.minRound));
  if (params.maxRound) qs.set('max_round', String(params.maxRound));
  if (params.txType && params.txType !== 'all') qs.set('tx_type', params.txType);
  if (params.policy && params.policy !== 'all') qs.set('policy', params.policy);
  if (params.minVolume) qs.set('min_volume', String(params.minVolume));
  if (params.minBalance) qs.set('min_balance', String(params.minBalance));
  return qs.toString();
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function parseSseFrame(frame) {
  let event = 'message';
  const data = [];
  for (const line of frame.split(/\r?\n/)) {
    if (!line || line.startsWith(':')) continue;
    const idx = line.indexOf(':');
    const field = idx === -1 ? line : line.slice(0, idx);
    const value = idx === -1 ? '' : line.slice(idx + 1).replace(/^ /, '');
    if (field === 'event') event = value || 'message';
    if (field === 'data') data.push(value);
  }
  if (data.length === 0) return null;
  const raw = data.join('\n');
  let parsed = raw;
  try {
    parsed = JSON.parse(raw);
  } catch {
    // Keep non-JSON keepalive text as-is.
  }
  return { event, data: parsed };
}

export function connectEvents({ onEvent, onError, onState } = {}) {
  let stopped = false;
  let controller = null;
  let retryMs = 1000;

  async function run() {
    while (!stopped) {
      if (!getToken()) return;
      controller = new AbortController();
      try {
        onState?.('connecting');
        const resp = await fetch('/api/events', {
          headers: {
            Accept: 'text/event-stream',
          },
          credentials: 'same-origin',
          signal: controller.signal,
        });
        if (resp.status === 401) {
          setToken(null);
          throw new Error('Session expired. Please log in again.');
        }
        if (!resp.ok || !resp.body) {
          throw new Error(`Event stream failed (${resp.status})`);
        }
        onState?.('connected');
        retryMs = 1000;
        const reader = resp.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        while (!stopped) {
          const { value, done } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const frames = buffer.split(/\r?\n\r?\n/);
          buffer = frames.pop() || '';
          for (const frame of frames) {
            const parsed = parseSseFrame(frame);
            if (parsed) onEvent?.(parsed);
          }
        }
      } catch (error) {
        if (!stopped) {
          onState?.('disconnected');
          onError?.(error);
          await sleep(retryMs);
          retryMs = Math.min(retryMs * 2, 30000);
        }
      }
    }
  }

  run();
  return {
    close() {
      stopped = true;
      controller?.abort();
    },
  };
}
