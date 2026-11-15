import { act, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { renderApp } from './helpers'
import { setupDevices } from './deviceFixtures'
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
