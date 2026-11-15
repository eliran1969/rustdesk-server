export const TOKEN_KEY = 'console_token'

export class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

export interface User {
  name: string
  display_name: string
  email: string
  note: string
  status: number
  is_admin: boolean
}

export interface Me extends User {
  must_change_password: boolean
}

export interface LoginResponse {
  type: string
  access_token: string
  user: User
}

export interface DeviceInfo {
  device_name?: string
  os?: string
  username?: string
  version?: string
  [key: string]: unknown
}

export interface Device {
  guid: string
  id: string
  status: number
  is_online: boolean
  last_online: string | null
  user_name: string | null
  note: string | null
  info: DeviceInfo | null
}

export interface DevicesPage {
  total: number
  data: Device[]
}

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

export function setToken(token: string): void {
  try {
    localStorage.setItem(TOKEN_KEY, token)
  } catch {
    // Storage unavailable: the session lasts until the page is closed.
  }
}

export function clearToken(): void {
  try {
    localStorage.removeItem(TOKEN_KEY)
  } catch {
    // Nothing to clear.
  }
}

let onUnauthorized: (() => void) | null = null

/** Lets the router-aware auth layer handle session expiry (defaults to a full page load). */
export function setUnauthorizedHandler(handler: (() => void) | null): void {
  onUnauthorized = handler
}

/**
 * Calls the console API. Adds the Bearer token from `localStorage['console_token']`.
 * A 401 on an authenticated request clears the token and goes to `/login`.
 * A body with `error` (or any non-2xx status) throws `ApiError`.
 */
export async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = getToken()
  const headers = new Headers(init.headers)
  if (token) headers.set('Authorization', `Bearer ${token}`)
  if (init.body !== undefined && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }
  let res: Response
  try {
    res = await fetch(path, { ...init, headers })
  } catch {
    throw new ApiError('network', 0)
  }
  let body: unknown = null
  const text = await res.text()
  if (text) {
    try {
      body = JSON.parse(text)
    } catch {
      body = null
    }
  }
  if (res.status === 401 && token) {
    clearToken()
    if (onUnauthorized) onUnauthorized()
    else window.location.assign('/login')
  }
  if (body !== null && typeof body === 'object' && 'error' in body) {
    throw new ApiError(String((body as { error: unknown }).error), res.status)
  }
  if (!res.ok) {
    throw new ApiError(`HTTP ${res.status}`, res.status)
  }
  return body as T
}

/** Maps an API failure to an i18n key so the user sees a translated message. */
export function errorKey(err: unknown): string {
  if (!(err instanceof ApiError)) return 'errors.generic'
  if (err.status === 0) return 'errors.network'
  if (err.status === 429) return 'errors.tooManyAttempts'
  if (err.message === 'Wrong username or password') return 'errors.wrongCredentials'
  if (err.message === 'Wrong old password') return 'errors.wrongOldPassword'
  if (err.message.startsWith('Password must be at least')) return 'errors.passwordTooShort'
  return 'errors.generic'
}
