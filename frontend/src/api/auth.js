import { request } from './client.js';

export const authApi = {
  getStatus: () => request('/api/status'),

  setup: (pin) =>
    request('/api/setup', {
      method: 'POST',
      body: JSON.stringify({ pin }),
    }),

  login: (pin) =>
    request('/api/login', {
      method: 'POST',
      body: JSON.stringify({ pin }),
    }),

  getSession: () => request('/api/session'),

  logout: () => request('/api/logout', { method: 'POST' }),

  changePin: (currentPin, newPin) =>
    request('/api/change-pin', {
      method: 'POST',
      body: JSON.stringify({ current_pin: currentPin, new_pin: newPin }),
    }),
};
