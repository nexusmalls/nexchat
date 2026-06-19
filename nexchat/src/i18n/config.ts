export const locales = ['en', 'ja', 'ko', 'es', 'zh', 'fr', 'de', 'pt', 'ru', 'ar'] as const;
export type Locale = (typeof locales)[number];

export const defaultLocale: Locale = 'en';

// Eagerly import default locale so the first render is synchronous
// (avoids blank screen in Capacitor WebView where dynamic import may stall).
import defaultMessages from '../../messages/en.json';
export { defaultMessages };

const messageLoaders: Record<Locale, () => Promise<Record<string, unknown>>> = {
  ar: () => import('../../messages/ar.json').then((mod) => mod.default),
  de: () => import('../../messages/de.json').then((mod) => mod.default),
  en: () => Promise.resolve(defaultMessages),
  es: () => import('../../messages/es.json').then((mod) => mod.default),
  fr: () => import('../../messages/fr.json').then((mod) => mod.default),
  ja: () => import('../../messages/ja.json').then((mod) => mod.default),
  ko: () => import('../../messages/ko.json').then((mod) => mod.default),
  pt: () => import('../../messages/pt.json').then((mod) => mod.default),
  ru: () => import('../../messages/ru.json').then((mod) => mod.default),
  zh: () => import('../../messages/zh.json').then((mod) => mod.default),
};

export function isLocale(value: string): value is Locale {
  return locales.includes(value as Locale);
}

export async function getMessages(locale: string = defaultLocale) {
  if (!isLocale(locale)) return defaultMessages;
  try {
    return await messageLoaders[locale]();
  } catch {
    return defaultMessages;
  }
}
