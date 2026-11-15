import { ADMIN, mockFetch } from './helpers'
import { USERS } from './orgFixtures'

export const DEVICES = [
  {
    guid: 'aa01',
    id: '111111111',
    status: 1,
    is_online: true,
    last_online: null,
    user_name: 'alice',
    group_name: 'Default',
    device_group_name: 'Servers',
    note: 'front desk',
    info: { device_name: 'PC-1', os: 'windows', username: 'bob', version: '1.4.0' },
  },
  {
    guid: 'bb02',
    id: '222222222',
    status: 0,
    is_online: false,
    last_online: null,
    user_name: '',
    group_name: '',
    device_group_name: '',
    note: '',
    info: { device_name: 'PC-2', os: 'linux', username: 'root', version: '1.4.0' },
  },
  {
    guid: 'cc03',
    id: '333333333',
    status: 1,
    is_online: false,
    last_online: null,
    user_name: '',
    group_name: '',
    device_group_name: '',
    note: '',
    info: { device_name: 'PC-3', os: 'macos', username: 'eve', version: '1.4.0' },
  },
]

export const DEVICE_GROUP_ROWS = [
  {
    guid: 'd1',
    name: 'Servers',
    note: 'racks',
    created_at: '2026-11-01 09:00:00',
    allowed_incomings: [{ type: 0, name: 'Sales' }],
    device_count: 1,
  },
]

export type Mutation = (
  path: string,
  method: string,
  body: unknown,
) => { status: number; body: unknown } | undefined

/** Fake backend for the devices and device groups pages; `mutate` may answer non-GET calls. */
export function setupDevices(me: Record<string, unknown> = ADMIN, mutate?: Mutation) {
  return mockFetch((url, init) => {
    const u = new URL(url, 'http://localhost')
    const method = init?.method ?? 'GET'
    if (u.pathname === '/api/console/me') {
      return { status: 200, body: { ...me, must_change_password: false } }
    }
    if (method !== 'GET') {
      const body = init?.body ? JSON.parse(String(init.body)) : undefined
      return mutate?.(u.pathname, method, body) ?? { status: 200, body: {} }
    }
    if (u.pathname === '/api/users') {
      return { status: 200, body: { total: USERS.length, data: USERS } }
    }
    if (u.pathname === '/api/device-groups') {
      return { status: 200, body: { total: DEVICE_GROUP_ROWS.length, data: DEVICE_GROUP_ROWS } }
    }
    if (u.pathname === '/api/user-groups') {
      return { status: 200, body: { total: 0, data: [] } }
    }
    if (u.pathname === '/api/devices') {
      const group = u.searchParams.get('device_group_name')
      const data = group ? DEVICES.filter((d) => d.device_group_name === group) : DEVICES
      return { status: 200, body: { total: data.length, data } }
    }
    return { status: 404, body: { error: 'Not found' } }
  })
}
