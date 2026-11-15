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

/** A translated error: i18n key, interpolation values, and the raw server text when unmapped. */
export interface ErrorInfo {
  key: string
  params?: Record<string, string>
  detail?: string
}

/** Server error strings with a fixed text → i18n key. */
const EXACT_ERRORS: Record<string, string> = {
  'Wrong username or password': 'errors.wrongCredentials',
  'Wrong old password': 'errors.wrongOldPassword',
  'User already exists': 'errors.userExists',
  'Disable the user first': 'errors.disableFirst',
  'Cannot change your own admin status': 'errors.selfProtected',
  'Cannot remove the last active administrator': 'errors.lastAdmin',
  'Name is required': 'errors.nameRequired',
  'Group already exists': 'errors.groupExists',
  'Group is not empty': 'errors.groupNotEmpty',
  'Cannot delete the default group': 'errors.defaultGroupDelete',
  'Cannot rename the default group': 'errors.defaultGroupRename',
  'Admin required!': 'errors.adminRequired',
}

/** Server error strings that end with a name → i18n key with `{{name}}`. */
const PREFIX_ERRORS: [string, string][] = [
  ['Unknown group or user: ', 'errors.unknownRuleTarget'],
  ['Unknown group: ', 'errors.unknownGroup'],
  ['Invalid rule type: ', 'errors.invalidRuleType'],
]

/** Maps an API failure to a translated message; unknown server text is kept as `detail`. */
export function describeError(err: unknown): ErrorInfo {
  if (!(err instanceof ApiError)) return { key: 'errors.generic' }
  if (err.status === 0) return { key: 'errors.network' }
  if (err.status === 429) return { key: 'errors.tooManyAttempts' }
  const exact = EXACT_ERRORS[err.message]
  if (exact) return { key: exact }
  if (err.message.startsWith('Password must be at least')) return { key: 'errors.passwordTooShort' }
  for (const [prefix, key] of PREFIX_ERRORS) {
    if (err.message.startsWith(prefix)) {
      return { key, params: { name: err.message.slice(prefix.length) } }
    }
  }
  if (err.status === 404) return { key: 'errors.notFound' }
  return { key: 'errors.generic', detail: err.message }
}

/** Maps an API failure to an i18n key so the user sees a translated message. */
export function errorKey(err: unknown): string {
  return describeError(err).key
}

/** Access rule target: 0 user group, 1 device group, 2 user. */
export interface Rule {
  type: number
  name: string
}

export interface ListResponse<T> {
  total: number
  data: T[]
}

export interface ManagedUser {
  guid: string
  name: string
  display_name: string
  email: string
  note: string
  status: number
  is_admin: boolean
  group_name: string | null
  created_at: string | null
}

export interface UserGroup {
  guid: string
  name: string
  note: string
  created_at: string | null
  allowed_incomings: Rule[]
  allowed_outgoings: Rule[]
  user_count: number
}

export interface DeviceGroup {
  guid: string
  name: string
  note?: string
}

/** The built-in group every user/device falls back to (matched case-insensitively). */
export function isDefaultGroup(name: string | null | undefined): boolean {
  return (name ?? '').toLowerCase() === 'default'
}

/** `?current&pageSize` plus the non-empty filters, for a list endpoint. */
export function listQuery(
  path: string,
  page: number,
  pageSize: number,
  filters: Record<string, string> = {},
): string {
  const params = new URLSearchParams({ current: String(page), pageSize: String(pageSize) })
  for (const [k, v] of Object.entries(filters)) {
    if (v) params.set(k, v)
  }
  return `${path}?${params.toString()}`
}
