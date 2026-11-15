import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth'
import { AppLayout } from './layout/AppLayout'
import { ChangePasswordPage } from './pages/ChangePasswordPage'
import { DeviceGroupsPage } from './pages/DeviceGroupsPage'
import { DevicesPage } from './pages/DevicesPage'
import { LoginPage } from './pages/LoginPage'
import { UserGroupsPage } from './pages/UserGroupsPage'
import { UsersPage } from './pages/UsersPage'

/** Requires a session; while a password change is pending only `/change-password` is reachable. */
function RequireAuth({ children, passwordPage = false }: { children: ReactNode; passwordPage?: boolean }) {
  const { t } = useTranslation()
  const { user, loading } = useAuth()
  if (loading) return <div className="page-loading">{t('app.loading')}</div>
  if (!user) return <Navigate to="/login" replace />
  if (user.must_change_password && !passwordPage) return <Navigate to="/change-password" replace />
  return children
}

/** Management pages: non-admins are sent back to the devices list. */
function RequireAdmin({ children }: { children: ReactNode }) {
  const { user } = useAuth()
  if (!user?.is_admin) return <Navigate to="/devices" replace />
  return children
}

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route
        path="/change-password"
        element={
          <RequireAuth passwordPage>
            <ChangePasswordPage />
          </RequireAuth>
        }
      />
      <Route
        element={
          <RequireAuth>
            <AppLayout />
          </RequireAuth>
        }
      >
        <Route path="/devices" element={<DevicesPage />} />
        <Route
          path="/users"
          element={
            <RequireAdmin>
              <UsersPage />
            </RequireAdmin>
          }
        />
        <Route
          path="/user-groups"
          element={
            <RequireAdmin>
              <UserGroupsPage />
            </RequireAdmin>
          }
        />
        <Route
          path="/device-groups"
          element={
            <RequireAdmin>
              <DeviceGroupsPage />
            </RequireAdmin>
          }
        />
      </Route>
      <Route path="*" element={<Navigate to="/devices" replace />} />
    </Routes>
  )
}

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <AppRoutes />
      </AuthProvider>
    </BrowserRouter>
  )
}
