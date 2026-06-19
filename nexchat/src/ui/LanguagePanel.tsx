import { useTranslations } from '@/i18n';
import { locales } from '@/i18n/config';
import { LOCALE_LABELS } from '@/i18n/locale-labels';
import { useLocaleStore } from '@/stores/locale-store';
import { useUiStore } from '@/state/uiStore';

// EN: Language picker — mirrors nexus-com-dapp settings language grid.
// CN: 语言选择页——对齐 nexus-com-dapp 设置页语言网格。
export function LanguagePanel() {
  const t = useTranslations('settings');
  const { locale, setLocale } = useLocaleStore();
  const setSettingsView = useUiStore((s) => s.setSettingsView);

  return (
    <main className="tg-main tg-settings-main">
      <header className="tg-sub-head">
        <button type="button" className="tg-sub-back" onClick={() => setSettingsView('list')}>
          {t('back')}
        </button>
        <span>{t('language')}</span>
      </header>
      <div className="tg-settings-detail">
        <p className="tg-settings-note">{t('languageDesc')}</p>
        <div className="tg-lang-grid">
          {locales.map((loc) => (
            <button
              key={loc}
              type="button"
              className={`tg-lang-btn${locale === loc ? ' active' : ''}`}
              onClick={() => setLocale(loc)}
            >
              {LOCALE_LABELS[loc]}
            </button>
          ))}
        </div>
      </div>
    </main>
  );
}
