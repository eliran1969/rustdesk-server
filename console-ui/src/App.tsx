import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'
import { AuthProvider, useAuth } from './auth'
import { AppLayout } from './layout/AppLayout'
import { ChangePasswordPage } from './pages/ChangePasswordPage'
import { DevicesPage } from './pages/DevicesPage'
import { LoginPage } from './pages/LoginPage'

/** Requires a session; while a password change is pending only `/change-password` is reachable. */
function RequireAuth({ children, passwordPage = false }: { children: ReactNode; passwordPage?: boolean }) {
  const { t } = useTranslation()
  const { user, loading } = useAuth()
  if (loading) return <div className="page-loading">{t('app.loading')}</div>
  if (!user) return <Navigate to="/login" replace />
  if (user.must_change_password && !passwordPage) return <Navigate to="/change-password" replace />
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
