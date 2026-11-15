import { useTranslation } from 'react-i18next'
import { NavLink, Outlet } from 'react-router-dom'
import { useAuth } from '../auth'
import { LanguageSwitch } from '../components/LanguageSwitch'
import { FolderIcon, LogoutIcon, MonitorIcon, UserIcon, UsersIcon } from '../components/Icons'

export function AppLayout() {
  const { t } = useTranslation()
  const { user, logout } = useAuth()
  const name = user?.display_name || user?.name || ''

  function onLogout() {
    void logout()
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <MonitorIcon />
          <span>{t('app.brand')}</span>
        </div>
        <nav aria-label={t('nav.main')}>
          <NavLink to="/devices" className="nav-item">
            <MonitorIcon />
            <span>{t('nav.devices')}</span>
          </NavLink>
          {user?.is_admin && (
            <>
              <NavLink to="/users" className="nav-item">
                <UserIcon />
                <span>{t('nav.users')}</span>
              </NavLink>
              <NavLink to="/user-groups" className="nav-item">
                <UsersIcon />
                <span>{t('nav.userGroups')}</span>
              </NavLink>
              <NavLink to="/device-groups" className="nav-item">
                <FolderIcon />
                <span>{t('nav.deviceGroups')}</span>
              </NavLink>
            </>
          )}
        </nav>
      </aside>
      <div className="main-col">
        <header className="topbar">
          <span className="user-name" title={t('header.signedInAs', { name })}>
            {name}
          </span>
          <LanguageSwitch />
          <button type="button" className="btn btn-ghost" onClick={onLogout}>
            <LogoutIcon />
            <span>{t('header.logout')}</span>
          </button>
        </header>
        <main className="content">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
