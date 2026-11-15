import { ADMIN, mockFetch } from './helpers'

export const USERS = [
  {
    guid: 'a1',
    name: 'alice',
    display_name: 'Alice A',
    email: 'alice@example.com',
    note: '',
    status: 1,
    is_admin: false,
    group_name: 'Default',
    created_at: '2026-11-01 10:00:00',
  },
  {
    guid: 'b2',
    name: 'bob',
    display_name: '',
    email: '',
    note: 'left',
    status: 0,
    is_admin: false,
    group_name: 'Sales',
    created_at: '2026-11-02 10:00:00',
  },
]

export const GROUPS = [
  {
    guid: 'g0',
    name: 'Default',
    note: '',
    created_at: '2026-11-01 09:00:00',
    allowed_incomings: [],
    allowed_outgoings: [],
    user_count: 1,
  },
  {
    guid: 'g1',
    name: 'Sales',
    note: 'sales team',
    created_at: '2026-11-01 09:30:00',
    allowed_incomings: [{ type: 2, name: 'alice' }],
    allowed_outgoings: [{ type: 1, name: 'Servers' }],
    user_count: 1,
  },
]

export const DEVICE_GROUPS = [{ guid: 'd1', name: 'Servers', note: '' }]

/** A fake console backend for the organization pages. */
export function setupOrg(me: Record<string, unknown> = ADMIN) {
  return mockFetch((url, init) => {
    const u = new URL(url, 'http://localhost')
    const method = init?.method ?? 'GET'
    if (u.pathname === '/api/console/me') {
      return { status: 200, body: { ...me, must_change_password: false } }
    }
    if (method !== 'GET') return { status: 200, body: {} }
    if (u.pathname === '/api/users') {
      return { status: 200, body: { total: USERS.length, data: USERS } }
    }
    if (u.pathname === '/api/user-groups') {
      return { status: 200, body: { total: GROUPS.length, data: GROUPS } }
    }
    if (u.pathname === '/api/device-groups') {
      return { status: 200, body: { total: DEVICE_GROUPS.length, data: DEVICE_GROUPS } }
    }
    if (u.pathname === '/api/devices') {
      return { status: 200, body: { total: 0, data: [] } }
    }
    return { status: 404, body: { error: 'Not found' } }
  })
}

export function callsTo(fetchMock: ReturnType<typeof mockFetch>, path: string, method = 'GET') {
  return fetchMock.mock.calls.filter(([u, init]) => {
    const p = new URL(String(u), 'http://localhost').pathname
    return p === path && (init?.method ?? 'GET') === method
  })
}
