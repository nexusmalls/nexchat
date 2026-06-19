import { create } from 'zustand';
import type { Locale } from '@/i18n/config';
import { locales, defaultLocale } from '@/i18n/config';

const STORAGE_KEY = 'nexchat_locale';
const INIT_KEY = 'nexchat_locale_initialized';

function loadLocale(): Locale {
  if (typeof window === 'undefined') return defaultLocale;
  try {
    const initialized = localStorage.getItem(INIT_KEY);
    if (!initialized) {
      localStorage.setItem(INIT_KEY, '1');
      return defaultLocale;
    }

    const saved = localStorage.getItem(STORAGE_KEY) as Locale | null;
    if (saved && locales.includes(saved)) return saved;
    if (saved) localStorage.setItem(STORAGE_KEY, defaultLocale);
  } catch {
    /* ignore */
  }
  return defaultLocale;
}

interface LocaleState {
  locale: Locale;
  _hydrated: boolean;
  _hydrate: () => void;
  setLocale: (locale: Locale) => void;
}

export const useLocaleStore = create<LocaleState>((set) => ({
  locale: defaultLocale,
  _hydrated: false,
  _hydrate: () => set({ locale: loadLocale(), _hydrated: true }),
  setLocale: (locale) => {
    if (typeof window !== 'undefined') {
      localStorage.setItem(INIT_KEY, '1');
      localStorage.setItem(STORAGE_KEY, locale);
    }
    set({ locale });
  },
}));
