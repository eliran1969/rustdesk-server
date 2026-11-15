import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { LanguageSwitch } from '../components/LanguageSwitch'
import { MonitorIcon } from '../components/Icons'

/** Centered card layout for the sign-in and forced password-change screens. */
export function AuthShell({ actions, children }: { actions?: ReactNode; children: ReactNode }) {
  const { t } = useTranslation()
  return (
    <div className="auth-shell">
      <header className="auth-bar">
        <span className="brand">
          <MonitorIcon />
          <span>{t('app.brand')}</span>
        </span>
        <div className="auth-bar-actions">
          <LanguageSwitch />
          {actions}
        </div>
      </header>
      <main className="auth-main">
        <div className="card auth-card">{children}</div>
      </main>
    </div>
  )
}
