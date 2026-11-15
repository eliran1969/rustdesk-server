import { act, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { ADMIN, mockFetch, renderApp } from './helpers'
import { DEVICES, setupDevices } from './deviceFixtures'
import { callsTo } from './orgFixtures'

function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{\{(\w+)\}\}/g, (_, k: string) => String(values[k]))
}

describe('DeviceGroupsPage', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'b'.repeat(64))
  })

  it('lists groups with device count and access rules', async () => {
    setupDevices()
    renderApp('/device-groups')
    const row = within((await screen.findByText('Servers')).closest('tr') as HTMLElement)
    expect(row.getByText('racks')).toBeInTheDocument()
    expect(row.getByText('1')).toBeInTheDocument()
    expect(row.getByText('Sales')).toBeInTheDocument()
  })

  it('add device by id posts array', async () => {
    const fetchMock = setupDevices()
    renderApp('/device-groups')
    const row = (await screen.findByText('Servers')).closest('tr') as HTMLElement
    fireEvent.click(within(row).getByRole('button', { name: en.deviceGroups.manageDevices }))
    const dialog = within(
      await screen.findByRole('dialog', {
        name: fill(en.deviceGroups.manageTitle, { name: 'Servers' }),
      }),
    )
    expect(await dialog.findByText('111111111')).toBeInTheDocument()
    const listed = callsTo(fetchMock, '/api/devices').map(([u]) =>
      new URL(String(u), 'http://localhost').searchParams.get('device_group_name'),
    )
    expect(listed).toContain('Servers')

    fireEvent.change(dialog.getByLabelText(en.deviceGroups.addIds), {
      target: { value: '444444444, 555555555  666666666,' },
    })
    fireEvent.click(dialog.getByRole('button', { name: en.deviceGroups.add }))
    await waitFor(() =>
      expect(callsTo(fetchMock, '/api/device-groups/d1', 'POST')).toHaveLength(1),
    )
    const [, init] = callsTo(fetchMock, '/api/device-groups/d1', 'POST')[0]
    expect(JSON.parse(String(init?.body))).toEqual(['444444444', '555555555', '666666666'])

    fireEvent.click(
      dialog.getByRole('button', {
        name: fill(en.deviceGroups.removeDevice, { id: '111111111' }),
      }),
    )
    await waitFor(() =>
      expect(callsTo(fetchMock, '/api/device-groups/d1/devices', 'DELETE')).toHaveLength(1),
    )
    const [, del] = callsTo(fetchMock, '/api/device-groups/d1/devices', 'DELETE')[0]
    expect(JSON.parse(String(del?.body))).toEqual(['111111111'])
  })

  it('manage devices keeps only exact group-name matches (LIKE wildcards)', async () => {
    const group = {
      guid: 'd9',
      name: 'Lab_1',
      note: '',
      created_at: null,
      allowed_incomings: [],
      device_count: 1,
    }
    const exact = { ...DEVICES[0], guid: 'e1', id: '700000001', device_group_name: 'Lab_1' }
    // `_` is a LIKE wildcard, so the server also returns a device of `LabX1`.
    const other = { ...DEVICES[2], guid: 'e2', id: '700000002', device_group_name: 'LabX1' }
    mockFetch((url) => {
      const u = new URL(url, 'http://localhost')
      if (u.pathname === '/api/console/me') {
        return { status: 200, body: { ...ADMIN, must_change_password: false } }
      }
      if (u.pathname === '/api/device-groups') {
        return { status: 200, body: { total: 1, data: [group] } }
      }
      if (u.pathname === '/api/devices') {
        return { status: 200, body: { total: 2, data: [exact, other] } }
      }
      return { status: 200, body: { total: 0, data: [] } }
    })
    renderApp('/device-groups')
    const row = (await screen.findByText('Lab_1')).closest('tr') as HTMLElement
    fireEvent.click(within(row).getByRole('button', { name: en.deviceGroups.manageDevices }))
    const dialog = within(
      await screen.findByRole('dialog', {
        name: fill(en.deviceGroups.manageTitle, { name: 'Lab_1' }),
      }),
    )
    expect(await dialog.findByText('700000001')).toBeInTheDocument()
    expect(dialog.queryByText('700000002')).not.toBeInTheDocument()
    expect(dialog.queryByText(/700000002/)).toBeNull()
  })

  it('unknown device id shows a translated error in the dialog', async () => {
    setupDevices(undefined, (path, method) =>
      path === '/api/device-groups/d1' && method === 'POST'
        ? { status: 400, body: { error: 'Unknown device: 999' } }
        : undefined,
    )
    renderApp('/device-groups')
    const row = (await screen.findByText('Servers')).closest('tr') as HTMLElement
    fireEvent.click(within(row).getByRole('button', { name: en.deviceGroups.manageDevices }))
    const dialog = within(await screen.findByRole('dialog'))
    fireEvent.change(dialog.getByLabelText(en.deviceGroups.addIds), { target: { value: '999' } })
    fireEvent.click(dialog.getByRole('button', { name: en.deviceGroups.add }))
    expect(await dialog.findByRole('alert')).toHaveTextContent(
      fill(en.errors.unknownDevice, { name: '999' }),
    )
  })

  it('create posts name, note and incoming rules; delete explains devices are removed', async () => {
    const fetchMock = setupDevices()
    renderApp('/device-groups')
    await screen.findByText('Servers')
    fireEvent.click(screen.getByRole('button', { name: en.deviceGroups.new }))
    const dialog = within(
      await screen.findByRole('dialog', { name: en.deviceGroups.createTitle }),
    )
    fireEvent.change(dialog.getByRole('textbox', { name: en.deviceGroups.fields.name }), {
      target: { value: 'Kiosks' },
    })
    const typeSelect = dialog.getByLabelText(en.rules.type) as HTMLSelectElement
    expect(Array.from(typeSelect.options).map((o) => o.text)).toEqual([
      en.rules.types.userGroup,
      en.rules.types.user,
    ])
    fireEvent.click(dialog.getByRole('button', { name: en.common.save }))
    await waitFor(() => expect(callsTo(fetchMock, '/api/device-groups', 'POST')).toHaveLength(1))
    const [, init] = callsTo(fetchMock, '/api/device-groups', 'POST')[0]
    expect(JSON.parse(String(init?.body))).toEqual({
      name: 'Kiosks',
      note: '',
      allowed_incomings: [],
    })
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20))
    })

    const row = screen.getByText('Servers').closest('tr') as HTMLElement
    fireEvent.click(within(row).getByRole('button', { name: en.common.delete }))
    const confirm = await screen.findByRole('dialog', {
      name: en.deviceGroups.confirmDeleteTitle,
    })
    expect(confirm).toHaveTextContent(
      fill(en.deviceGroups.confirmDeleteWithDevices, { name: 'Servers', count: 1 }),
    )
  })
})
