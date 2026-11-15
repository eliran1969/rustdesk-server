import { useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { Link, useNavigate } from 'react-router-dom'
import { apiFetch, errorKey } from '../api'
import { useAuth } from '../auth'
import { LogoutIcon } from '../components/Icons'
import { AuthShell } from '../layout/AuthShell'

const MIN_PASSWORD_LEN = 8

export function ChangePasswordPage() {
  const { t } = useTranslation()
  const { user, logout, passwordChanged } = useAuth()
  const navigate = useNavigate()
  const [oldPassword, setOldPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const forced = user?.must_change_password ?? false

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    setError('')
    if ([...newPassword].length < MIN_PASSWORD_LEN) {
      setError('errors.passwordTooShort')
      return
    }
    if (newPassword !== confirm) {
      setError('changePassword.mismatch')
      return
    }
    setBusy(true)
    try {
      await apiFetch('/api/console/change-password', {
        method: 'POST',
        body: JSON.stringify({ old_password: oldPassword, new_password: newPassword }),
      })
      passwordChanged()
      navigate('/devices', { replace: true })
    } catch (err) {
      setError(errorKey(err))
      setBusy(false)
    }
  }

  function onLogout() {
    void logout()
  }

  const logoutButton = (
    <button type="button" className="btn btn-ghost" onClick={onLogout}>
      <LogoutIcon />
      <span>{t('header.logout')}</span>
    </button>
  )

  return (
    <AuthShell actions={logoutButton}>
      <h1 className="auth-title">{t('changePassword.title')}</h1>
      {forced && <p className="notice">{t('changePassword.forced')}</p>}
      <form className="form" onSubmit={onSubmit} noValidate>
        <label className="field">
          <span>{t('changePassword.oldPassword')}</span>
          <input
            type="password"
            autoComplete="current-password"
            value={oldPassword}
            onChange={(e) => setOldPassword(e.target.value)}
            required
            autoFocus
          />
        </label>
        <label className="field">
          <span>{t('changePassword.newPassword')}</span>
          <input
            type="password"
            autoComplete="new-password"
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
            minLength={MIN_PASSWORD_LEN}
            required
          />
          <small className="muted">{t('changePassword.hint')}</small>
        </label>
        <label className="field">
          <span>{t('changePassword.confirmPassword')}</span>
          <input
            type="password"
            autoComplete="new-password"
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
            required
          />
        </label>
        {error && (
          <div className="alert" role="alert">
            {t(error)}
          </div>
        )}
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? t('changePassword.submitting') : t('changePassword.submit')}
        </button>
        {!forced && (
          <Link to="/devices" className="link-back">
            {t('changePassword.back')}
          </Link>
        )}
      </form>
    </AuthShell>
  )
}
