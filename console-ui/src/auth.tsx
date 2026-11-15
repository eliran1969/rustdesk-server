import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  apiFetch,
  clearToken,
  getToken,
  setToken,
  setUnauthorizedHandler,
} from './api'
import type { LoginResponse, Me } from './api'

interface AuthValue {
  user: Me | null
  loading: boolean
  login(username: string, password: string): Promise<Me>
  logout(): Promise<void>
  passwordChanged(): void
}

const AuthContext = createContext(null as AuthValue | null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const navigate = useNavigate()
  const [user, setUser] = useState(null as Me | null)
  const [loading, setLoading] = useState(getToken() !== null)

  useEffect(() => {
    setUnauthorizedHandler(function onExpired() {
      setUser(null)
      navigate('/login', { replace: true })
    })
    return function unregister() {
      setUnauthorizedHandler(null)
    }
  }, [navigate])

  useEffect(() => {
    if (getToken() === null) return
    let alive = true
    apiFetch('/api/console/me')
      .then(function loaded(me) {
        if (alive) setUser(me as Me)
      })
      .catch(function failed() {
        if (alive) setUser(null)
      })
      .finally(function done() {
        if (alive) setLoading(false)
      })
    return function cancel() {
      alive = false
    }
  }, [])

  const login = useCallback(async function login(username: string, password: string) {
    const res = (await apiFetch('/api/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    })) as LoginResponse
    setToken(res.access_token)
    const me = (await apiFetch('/api/console/me')) as Me
    setUser(me)
    return me
  }, [])

  const logout = useCallback(async function logout() {
    try {
      await apiFetch('/api/logout', { method: 'POST' })
    } catch {
      // The session is dropped locally either way.
    }
    clearToken()
    setUser(null)
    navigate('/login', { replace: true })
  }, [navigate])

  const passwordChanged = useCallback(function passwordChanged() {
    setUser(function update(u) {
      return u ? { ...u, must_change_password: false } : u
    })
  }, [])

  const value = useMemo(
    function build() {
      return { user, loading, login, logout, passwordChanged }
    },
    [user, loading, login, logout, passwordChanged],
  )
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth(): AuthValue {
  const ctx = useContext(AuthContext)
  if (ctx === null) throw new Error('useAuth must be used inside AuthProvider')
  return ctx
}

/** Where a signed-in user belongs: the forced password change, or the devices list. */
export function homePath(user: Me): string {
  return user.must_change_password ? '/change-password' : '/devices'
}
