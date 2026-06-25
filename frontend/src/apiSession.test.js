import assert from 'node:assert/strict';
import test from 'node:test';

import { api, getToken, setToken } from './api.js';

test('session state is kept in memory instead of localStorage', () => {
  const calls = [];
  globalThis.localStorage = {
    getItem(key) {
      calls.push(['getItem', key]);
      return null;
    },
    setItem(key, value) {
      calls.push(['setItem', key, value]);
    },
    removeItem(key) {
      calls.push(['removeItem', key]);
    },
  };

  setToken('server-cookie');
  assert.equal(getToken(), 'cookie-session');
  setToken(null);
  assert.equal(getToken(), null);
  assert.deepEqual(calls, []);
});

test('state-changing API requests use cookie credentials and CSRF header', async () => {
  setToken('server-cookie');
  const originalFetch = globalThis.fetch;
  let seen;
  globalThis.fetch = async (path, options) => {
    seen = { path, options };
    return new Response('{}', { status: 200 });
  };

  try {
    await api.logout();
  } finally {
    globalThis.fetch = originalFetch;
    setToken(null);
  }

  assert.equal(seen.path, '/api/logout');
  assert.equal(seen.options.credentials, 'same-origin');
  assert.equal(seen.options.headers.Authorization, undefined);
  assert.equal(seen.options.headers['X-OpenNodia-CSRF'], '1');
});

test('SSE connects with cookie credentials and no bearer token', () => {
  setToken('server-cookie');
  const originalFetch = globalThis.fetch;
  let seen;
  globalThis.fetch = (path, options) => {
    seen = { path, options };
    return new Promise(() => {});
  };

  const stream = api.connectEvents();
  stream.close();
  globalThis.fetch = originalFetch;
  setToken(null);

  assert.equal(seen.path, '/api/events');
  assert.equal(seen.options.credentials, 'same-origin');
  assert.equal(seen.options.headers.Authorization, undefined);
});
