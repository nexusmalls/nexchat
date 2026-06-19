import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { useLocaleStore } from '@/stores/locale-store';
import {
  defaultLocale,
  defaultMessages,
  getMessages,
  type Locale,
} from '@/i18n/config';

type Messages = Record<string, unknown>;

type TranslateFn = (key: string, values?: Record<string, string | number>) => string;

interface IntlContextValue {
  locale: Locale;
  messages: Messages;
}

const IntlContext = createContext<IntlContextValue>({
  locale: defaultLocale,
  messages: defaultMessages,
});

function lookup(messages: Messages, namespace: string, key: string): string | undefined {
  const root = namespace ? (messages[namespace] as Messages | undefined) : messages;
  if (!root) return undefined;
  const value = key.split('.').reduce<unknown>((obj, part) => {
    if (obj && typeof obj === 'object' && part in (obj as Messages)) {
      return (obj as Messages)[part];
    }
    return undefined;
  }, root);
  return typeof value === 'string' ? value : undefined;
}

function interpolate(template: string, values?: Record<string, string | number>): string {
  if (!values) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => {
    const v = values[name];
    return v == null ? `{${name}}` : String(v);
  });
}

function makeTranslator(messages: Messages, namespace: string): TranslateFn {
  return (key, values) => {
    const text = lookup(messages, namespace, key) ?? key;
    return interpolate(text, values);
  };
}

export function IntlProvider({ children }: { children: ReactNode }) {
  const locale = useLocaleStore((s) => s.locale);
  const [messages, setMessages] = useState<Messages>(defaultMessages);
  const [currentLocale, setCurrentLocale] = useState<Locale>(defaultLocale);

  useEffect(() => {
    useLocaleStore.getState()._hydrate();
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  useEffect(() => {
    if (locale === defaultLocale) {
      setMessages(defaultMessages);
      setCurrentLocale(defaultLocale);
      return;
    }
    void getMessages(locale).then((msgs) => {
      setMessages(msgs);
      setCurrentLocale(locale);
    });
  }, [locale]);

  const value = useMemo(
    () => ({ locale: currentLocale, messages }),
    [currentLocale, messages],
  );

  return <IntlContext.Provider value={value}>{children}</IntlContext.Provider>;
}

export function useTranslations(namespace?: string): TranslateFn {
  const { messages } = useContext(IntlContext);
  return useMemo(() => makeTranslator(messages, namespace ?? ''), [messages, namespace]);
}
