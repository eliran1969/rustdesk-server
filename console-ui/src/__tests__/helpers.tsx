import { render } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { vi } from 'vitest'
import { AppRoutes } from '../App'
import { AuthProvider } from '../auth'

export type Handler = (url: string, init?: RequestInit) => { status: number; body: unknown }

export function mockFetch(handler: Handler) {
  const fn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url
    const { status, body } = handler(url, init)
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    })
  })
  vi.stubGlobal('fetch', fn)
  return fn
}

export function renderApp(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AuthProvider>
        <AppRoutes />
      </AuthProvider>
    </MemoryRouter>,
  )
}

export const ADMIN = {
  name: 'admin',
  display_name: 'Admin',
  email: '',
  note: '',
  status: 1,
  is_admin: true,
}
