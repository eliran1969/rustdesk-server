import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DeviceEditDrawer } from '../components/DeviceEditDrawer'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { DEVICES, setupDevices } from './deviceFixtures'
import { callsTo } from './orgFixtures'

describe('DeviceEditDrawer', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'd'.repeat(64))
  })

  function open(onSaved = vi.fn()) {
    render(
      <DeviceEditDrawer
        device={DEVICES[0]}
        users={['alice', 'bob']}
        deviceGroups={['Servers']}
        onClose={vi.fn()}
        onSaved={onSaved}
      />,
    )
    return within(screen.getByRole('dialog', { name: en.devices.edit.title }))
  }

  it('empty user unassigns', async () => {
    const fetchMock = setupDevices()
    const onSaved = vi.fn()
    const d = open(onSaved)
    const user = d.getByLabelText(en.devices.edit.user) as HTMLSelectElement
    expect(user.value).toBe('alice')
    expect(d.getByRole('option', { name: en.devices.noUser })).toHaveValue('')
    fireEvent.change(user, { target: { value: '' } })
    fireEvent.click(d.getByRole('button', { name: en.common.save }))
    await waitFor(() => expect(onSaved).toHaveBeenCalled())
    const calls = callsTo(fetchMock, '/api/devices/aa01', 'PATCH')
    expect(calls).toHaveLength(1)
    expect(JSON.parse(String(calls[0][1]?.body))).toEqual({ user_name: '' })
  })

  it('prefills overrides and resets the name to the reported one', async () => {
    const fetchMock = setupDevices()
    const onSaved = vi.fn()
    const d = open(onSaved)
    expect(d.getByLabelText(en.devices.edit.deviceName)).toHaveValue('PC-1')
    expect(d.getByLabelText(en.devices.edit.osUser)).toHaveValue('bob')
    expect(d.getByLabelText(en.devices.edit.deviceGroup)).toHaveValue('Servers')
    fireEvent.click(
      d.getByRole('button', {
        name: en.devices.edit.resetField.replace('{{field}}', en.devices.edit.deviceName),
      }),
    )
    fireEvent.change(d.getByLabelText(en.devices.edit.note), { target: { value: 'lobby' } })
    fireEvent.click(d.getByRole('button', { name: en.common.save }))
    await waitFor(() => expect(onSaved).toHaveBeenCalled())
    const calls = callsTo(fetchMock, '/api/devices/aa01', 'PATCH')
    expect(JSON.parse(String(calls[0][1]?.body))).toEqual({ device_name: '', note: 'lobby' })
  })

  it('explains what disable and delete do not do yet', () => {
    setupDevices()
    const d = open()
    expect(d.getByText(en.devices.disableHint)).toBeInTheDocument()
    expect(d.getByText(en.devices.deleteOnlineHint)).toBeInTheDocument()
  })

  it('shows a translated error when the assigned user is disabled', async () => {
    setupDevices(undefined, () => ({
      status: 400,
      body: { error: 'User is disabled: bob' },
    }))
    const d = open()
    fireEvent.change(d.getByLabelText(en.devices.edit.user), { target: { value: 'bob' } })
    fireEvent.click(d.getByRole('button', { name: en.common.save }))
    expect(await d.findByRole('alert')).toHaveTextContent(
      en.errors.userDisabled.replace('{{name}}', 'bob'),
    )
  })

  it('shows a translated error when the server rejects the group', async () => {
    setupDevices(undefined, () => ({
      status: 400,
      body: { error: 'Unknown device group: Servers' },
    }))
    const d = open()
    fireEvent.change(d.getByLabelText(en.devices.edit.note), { target: { value: 'x' } })
    fireEvent.click(d.getByRole('button', { name: en.common.save }))
    expect(await d.findByRole('alert')).toHaveTextContent(
      en.errors.unknownDeviceGroup.replace('{{name}}', 'Servers'),
    )
  })
})
