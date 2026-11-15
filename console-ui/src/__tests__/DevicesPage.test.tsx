import { fireEvent, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { ADMIN, mockFetch, renderApp } from './helpers'

function deviceCalls(fetchMock: ReturnType<typeof mockFetch>) {
  return fetchMock.mock.calls
    .map(([u]) => new URL(String(u), 'http://localhost'))
    .filter((u) => u.pathname === '/api/devices')
}

function setup(devices: unknown[] = [], total = devices.length) {
  return mockFetch((url) => {
    if (url.endsWith('/api/console/me')) {
      return { status: 200, body: { ...ADMIN, must_change_password: false } }
    }
    if (url.includes('/api/devices')) {
      return { status: 200, body: { total, data: devices } }
    }
    return { status: 404, body: { error: 'Not found' } }
  })
}

describe('DevicesPage', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'c'.repeat(64))
  })

  it('wraps search term with % and sends device_name', async () => {
    const fetchMock = setup()
    renderApp('/devices')
    await screen.findByText(en.devices.empty)
    fireEvent.change(screen.getByLabelText(en.devices.searchBy), {
      target: { value: 'device_name' },
    })
    fireEvent.change(screen.getByLabelText(en.devices.searchPlaceholder), {
      target: { value: 'abc' },
    })
    fireEvent.click(screen.getByRole('button', { name: en.devices.search }))
    await waitFor(() => {
      expect(deviceCalls(fetchMock).at(-1)?.searchParams.get('device_name')).toBe('%abc%')
    })
    const last = deviceCalls(fetchMock).at(-1)
    expect(last?.searchParams.has('id')).toBe(false)
    expect(last?.searchParams.get('current')).toBe('1')
  })

  it('keeps an explicit % pattern and searches by id', async () => {
    const fetchMock = setup()
    renderApp('/devices')
    await screen.findByText(en.devices.empty)
    fireEvent.change(screen.getByLabelText(en.devices.searchPlaceholder), {
      target: { value: '12%' },
    })
    fireEvent.click(screen.getByRole('button', { name: en.devices.search }))
    await waitFor(() => {
      expect(deviceCalls(fetchMock).at(-1)?.searchParams.get('id')).toBe('12%')
    })
  })

  it('renders device rows with online status and disabled badge', async () => {
    setup([
      {
        guid: 'g1',
        id: '123456789',
        status: 0,
        is_online: true,
        last_online: null,
        user_name: 'alice',
        note: 'front desk',
        info: { device_name: 'PC-1', os: 'windows', username: 'bob', version: '1.4.0' },
      },
    ])
    renderApp('/devices')
    const row = (await screen.findByText('123456789')).closest('tr')
    expect(row).not.toBeNull()
    const cells = within(row as HTMLElement)
    expect(cells.getByText('PC-1')).toBeInTheDocument()
    expect(cells.getByText(en.devices.disabled)).toBeInTheDocument()
    expect(cells.getByLabelText(en.devices.online)).toBeInTheDocument()
  })
})
