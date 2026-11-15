import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import en from './en.json'
import he from './he.json'

export const LANG_KEY = 'console_lang'
export type Lang = 'he' | 'en'

function storedLanguage(): Lang | null {
  try {
    const v = localStorage.getItem(LANG_KEY)
    return v === 'he' || v === 'en' ? v : null
  } catch {
    return null
  }
}

/** Saved choice first, then the browser language (`he*` → he, anything else → en). */
export function detectLanguage(): Lang {
  const saved = storedLanguage()
  if (saved) return saved
  const browser = (navigator.languages?.[0] ?? navigator.language ?? '').toLowerCase()
  return browser.startsWith('he') || browser.startsWith('iw') ? 'he' : 'en'
}

export function normalizeLanguage(lng: string | undefined): Lang {
  return lng?.startsWith('he') ? 'he' : 'en'
}

function applyDocumentLanguage(lng: string | undefined) {
  const lang = normalizeLanguage(lng)
  document.documentElement.lang = lang
  document.documentElement.dir = lang === 'he' ? 'rtl' : 'ltr'
}

i18n.on('languageChanged', applyDocumentLanguage)

void i18n.use(initReactI18next).init({
  resources: { he: { translation: he }, en: { translation: en } },
  lng: detectLanguage(),
  fallbackLng: 'en',
  supportedLngs: ['he', 'en'],
  interpolation: { escapeValue: false },
  initAsync: false,
})
applyDocumentLanguage(i18n.language)

export async function setLanguage(lng: Lang): Promise<void> {
  try {
    localStorage.setItem(LANG_KEY, lng)
  } catch {
    // Storage unavailable (private mode): the choice simply is not remembered.
  }
  await i18n.changeLanguage(lng)
  applyDocumentLanguage(lng)
}

export default i18n
