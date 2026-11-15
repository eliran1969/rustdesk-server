import { fireEvent, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { setupDevices } from './deviceFixtures'
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

function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{\{(\w+)\}\}/g, (_, k: string) => String(values[k]))
}

function rowOfId(id: string): HTMLElement {
  const row = screen.getByText(id).closest('tr')
  expect(row).not.toBeNull()
  return row as HTMLElement
}

function selectBox(id: string): HTMLElement {
  return within(rowOfId(id)).getByRole('checkbox', {
    name: fill(en.devices.selectRow, { id }),
  })
}

describe('DevicesPage (admin)', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'c'.repeat(64))
  })

  it('batch assign sends one request per selected device and shows summary', async () => {
    const fetchMock = setupDevices(undefined, (path, method) =>
      path === '/api/devices/cc03' && method === 'PATCH'
        ? { status: 400, body: { error: 'Unknown user: alice' } }
        : undefined,
    )
    renderApp('/devices')
    await screen.findByText('111111111')
    fireEvent.click(selectBox('111111111'))
    fireEvent.click(selectBox('333333333'))
    expect(screen.getByText(fill(en.devices.selected, { count: 2 }))).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: en.devices.batch.assignUser }))
    const dialog = within(
      await screen.findByRole('dialog', {
        name: fill(en.devices.batch.assignUserTitle, { count: 2 }),
      }),
    )
    await dialog.findByRole('option', { name: 'alice' })
    fireEvent.change(dialog.getByLabelText(en.devices.edit.user), { target: { value: 'alice' } })
    fireEvent.click(dialog.getByRole('button', { name: en.devices.batch.apply }))

    expect(
      await screen.findByText(fill(en.devices.batch.summary, { ok: 1, failed: 1 })),
    ).toBeInTheDocument()
    expect(
      screen.getByText(fill(en.devices.batch.failedIds, { ids: '333333333' })),
    ).toBeInTheDocument()
    const patches = fetchMock.mock.calls.filter(([, init]) => init?.method === 'PATCH')
    expect(patches.map(([u]) => new URL(String(u), 'http://localhost').pathname)).toEqual([
      '/api/devices/aa01',
      '/api/devices/cc03',
    ])
    for (const [, init] of patches) {
      expect(JSON.parse(String(init?.body))).toEqual({ user_name: 'alice' })
    }
    // The failed device stays selected so the action can be retried.
    expect(screen.getByText(fill(en.devices.selected, { count: 1 }))).toBeInTheDocument()
  })

  it('selection resets when the device group filter changes', async () => {
    setupDevices()
    renderApp('/devices')
    await screen.findByText('111111111')
    fireEvent.click(selectBox('111111111'))
    expect(screen.getByText(fill(en.devices.selected, { count: 1 }))).toBeInTheDocument()
    const filter = screen.getByLabelText(en.devices.filterDeviceGroup)
    await within(filter).findByRole('option', { name: 'Servers' })
    fireEvent.change(filter, { target: { value: 'Servers' } })
    await waitFor(() =>
      expect(
        screen.queryByText(fill(en.devices.selected, { count: 1 })),
      ).not.toBeInTheDocument(),
    )
  })

  it('delete disabled only', async () => {
    const fetchMock = setupDevices()
    renderApp('/devices')
    await screen.findByText('111111111')
    const enabledDelete = within(rowOfId('111111111')).getByRole('button', {
      name: en.common.delete,
    })
    expect(enabledDelete).toBeDisabled()
    expect(enabledDelete).toHaveAttribute('title', en.devices.deleteEnabledHint)

    const disabledDelete = within(rowOfId('222222222')).getByRole('button', {
      name: en.common.delete,
    })
    expect(disabledDelete).toBeEnabled()
    fireEvent.click(disabledDelete)
    const dialog = await screen.findByRole('dialog', { name: en.devices.confirmDeleteTitle })
    fireEvent.click(within(dialog).getByRole('button', { name: en.common.delete }))
    await waitFor(() =>
      expect(fetchMock.mock.calls.filter(([, init]) => init?.method === 'DELETE')).toHaveLength(1),
    )
    const [url] = fetchMock.mock.calls.filter(([, init]) => init?.method === 'DELETE')[0]
    expect(new URL(String(url), 'http://localhost').pathname).toBe('/api/devices/bb02')
  })

  it('non-admin sees no selection or actions', async () => {
    setupDevices({ ...ADMIN, name: 'joe', is_admin: false })
    renderApp('/devices')
    const row = within((await screen.findByText('111111111')).closest('tr') as HTMLElement)
    expect(row.getByText('Servers')).toBeInTheDocument()
    expect(row.queryByRole('checkbox')).not.toBeInTheDocument()
    expect(row.queryByRole('button')).not.toBeInTheDocument()
    expect(screen.queryByLabelText(en.devices.filterDeviceGroup)).not.toBeInTheDocument()
  })
})
