import { writable, derived } from 'svelte/store';
import { en } from './en.js';
import { ko } from './ko.js';
import { zh } from './zh.js';
import { ja } from './ja.js';

const dictionaries = { en, ko, zh, ja };

export const locales = [
  { code: 'en', label: 'English', short: 'EN' },
  { code: 'ko', label: '한국어', short: 'KO' },
  { code: 'zh', label: '中文', short: 'ZH' },
  { code: 'ja', label: '日本語', short: 'JA' },
];

const STORAGE_KEY = 'opennodia_locale';

function getInitialLocale() {
  // Default to English; once the user picks a language it persists.
  const stored = typeof localStorage !== 'undefined' && localStorage.getItem(STORAGE_KEY);
  if (stored && dictionaries[stored]) return stored;
  return 'en';
}

export const locale = writable(getInitialLocale());

locale.subscribe((val) => {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, val);
  }
});

export function setLocale(code) {
  if (dictionaries[code]) {
    locale.set(code);
  }
}

function flatten(obj, prefix = '') {
  const result = {};
  for (const [key, val] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    if (typeof val === 'object' && val !== null) {
      Object.assign(result, flatten(val, fullKey));
    } else {
      result[fullKey] = val;
    }
  }
  return result;
}

const flatDictionaries = {};
for (const [code, dict] of Object.entries(dictionaries)) {
  flatDictionaries[code] = flatten(dict);
}

export const t = derived(locale, ($locale) => {
  const dict = flatDictionaries[$locale] || flatDictionaries.en;
  const fallback = flatDictionaries.en;
  return (key, params = null) => {
    let str = dict[key] ?? fallback[key] ?? key;
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        str = str.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
      }
    }
    return str;
  };
});
