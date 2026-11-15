import { fireEvent, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import he from '../i18n/he.json'
import { ADMIN, mockFetch, renderApp } from './helpers'

function submitLogin() {
  fireEvent.change(screen.getByLabelText(he.login.username), { target: { value: 'admin' } })
  fireEvent.change(screen.getByLabelText(he.login.password), { target: { value: 'secret' } })
  fireEvent.click(screen.getByRole('button', { name: he.login.submit }))
}

describe('LoginPage', () => {
  beforeEach(async () => {
    await setLanguage('he')
  })

  it('shows server error message on 401', async () => {
    mockFetch(() => ({ status: 401, body: { error: 'Wrong username or password' } }))
    renderApp('/login')
    submitLogin()
    expect(await screen.findByRole('alert')).toHaveTextContent(he.errors.wrongCredentials)
    expect(screen.getByLabelText(he.login.username)).toBeInTheDocument()
  })

  it('shows rate-limit message on 429', async () => {
    mockFetch(() => ({ status: 429, body: { error: 'Too many requests' } }))
    renderApp('/login')
    submitLogin()
    expect(await screen.findByRole('alert')).toHaveTextContent(he.errors.tooManyAttempts)
  })

  it('redirects to /change-password when must_change_password', async () => {
    const token = 'a'.repeat(64)
    const fetchMock = mockFetch((url) => {
      if (url.endsWith('/api/login')) {
        return { status: 200, body: { type: 'access_token', access_token: token, user: ADMIN } }
      }
      if (url.endsWith('/api/console/me')) {
        return { status: 200, body: { ...ADMIN, must_change_password: true } }
      }
      return { status: 404, body: { error: 'Not found' } }
    })
    renderApp('/login')
    submitLogin()
    expect(
      await screen.findByRole('heading', { name: he.changePassword.title }),
    ).toBeInTheDocument()
    expect(localStorage.getItem('console_token')).toBe(token)
    const meCall = fetchMock.mock.calls.find(([u]) => String(u).endsWith('/api/console/me'))
    expect(meCall).toBeDefined()
    const headers = new Headers(meCall?.[1]?.headers)
    expect(headers.get('Authorization')).toBe(`Bearer ${token}`)
  })

  it('cannot leave /change-password while must_change_password', async () => {
    localStorage.setItem('console_token', 'b'.repeat(64))
    mockFetch((url) => {
      if (url.endsWith('/api/console/me')) {
        return { status: 200, body: { ...ADMIN, must_change_password: true } }
      }
      return { status: 200, body: { total: 0, data: [] } }
    })
    renderApp('/devices')
    expect(
      await screen.findByRole('heading', { name: he.changePassword.title }),
    ).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: he.nav.devices })).not.toBeInTheDocument()
  })

  it('clears token and returns to login when /me answers 401', async () => {
    localStorage.setItem('console_token', 'd'.repeat(64))
    mockFetch(() => ({ status: 401, body: { error: 'Unauthorized' } }))
    renderApp('/devices')
    expect(await screen.findByLabelText(he.login.username)).toBeInTheDocument()
    expect(localStorage.getItem('console_token')).toBeNull()
  })
})
