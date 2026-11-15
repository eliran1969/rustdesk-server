import { useTranslation } from 'react-i18next'
import { normalizeLanguage, setLanguage } from '../i18n'

export function LanguageSwitch() {
  const { t, i18n } = useTranslation()
  const current = normalizeLanguage(i18n.language)

  function toHebrew() {
    void setLanguage('he')
  }

  function toEnglish() {
    void setLanguage('en')
  }

  return (
    <div className="lang-switch" role="group" aria-label={t('header.language')}>
      <button
        type="button"
        lang="he"
        className={current === 'he' ? 'active' : ''}
        aria-pressed={current === 'he'}
        onClick={toHebrew}
      >
        {t('header.hebrew')}
      </button>
      <button
        type="button"
        lang="en"
        className={current === 'en' ? 'active' : ''}
        aria-pressed={current === 'en'}
        onClick={toEnglish}
      >
        {t('header.english')}
      </button>
    </div>
  )
}
