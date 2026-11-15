import { useState } from 'react'
import type { ChangeEvent, FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { Navigate, useNavigate } from 'react-router-dom'
import { errorKey } from '../api'
import { homePath, useAuth } from '../auth'
import { AuthShell } from '../layout/AuthShell'

export function LoginPage() {
  const { t } = useTranslation()
  const { user, loading, login } = useAuth()
  const navigate = useNavigate()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  if (loading) return <div className="page-loading">{t('app.loading')}</div>
  if (user) return <Navigate to={homePath(user)} replace />

  function onUsername(e: ChangeEvent<HTMLInputElement>) {
    setUsername(e.target.value)
  }

  function onPassword(e: ChangeEvent<HTMLInputElement>) {
    setPassword(e.target.value)
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError('')
    setBusy(true)
    try {
      const me = await login(username.trim(), password)
      navigate(homePath(me), { replace: true })
    } catch (err) {
      setError(errorKey(err))
      setBusy(false)
    }
  }

  return (
    <AuthShell>
      <h1 className="auth-title">{t('login.title')}</h1>
      <p className="muted">{t('login.subtitle')}</p>
      <form className="form" onSubmit={onSubmit} noValidate>
        <label className="field">
          <span>{t('login.username')}</span>
          <input
            name="username"
            autoComplete="username"
            value={username}
            onChange={onUsername}
            required
            autoFocus
          />
        </label>
        <label className="field">
          <span>{t('login.password')}</span>
          <input
            name="password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={onPassword}
            required
          />
        </label>
        {error && (
          <div className="alert" role="alert">
            {t(error)}
          </div>
        )}
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? t('login.submitting') : t('login.submit')}
        </button>
      </form>
    </AuthShell>
  )
}
