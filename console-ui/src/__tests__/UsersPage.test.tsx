import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { ADMIN, renderApp } from './helpers'
import { callsTo, setupOrg } from './orgFixtures'

function rowOf(text: string): HTMLElement {
  const row = screen.getByText(text).closest('tr')
  expect(row).not.toBeNull()
  return row as HTMLElement
}

describe('UsersPage', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'e'.repeat(64))
  })

  it('admin sees users menu, non-admin does not', async () => {
    setupOrg()
    renderApp('/devices')
    expect(await screen.findByRole('link', { name: en.nav.users })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: en.nav.userGroups })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: en.nav.deviceGroups })).toBeInTheDocument()
    cleanup()

    setupOrg({ ...ADMIN, name: 'joe', is_admin: false })
    renderApp('/users')
    expect(await screen.findByRole('heading', { name: en.devices.title })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: en.nav.devices })).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: en.nav.users })).not.toBeInTheDocument()
    expect(screen.queryByRole('link', { name: en.nav.userGroups })).not.toBeInTheDocument()
    expect(screen.queryByRole('link', { name: en.nav.deviceGroups })).not.toBeInTheDocument()
  })

  it('create user posts expected body and refreshes', async () => {
    const fetchMock = setupOrg()
    renderApp('/users')
    await screen.findByText('bob')
    const listsBefore = callsTo(fetchMock, '/api/users').length

    fireEvent.click(screen.getByRole('button', { name: en.users.new }))
    const dialog = await screen.findByRole('dialog', { name: en.users.createTitle })
    const d = within(dialog)
    fireEvent.change(d.getByLabelText(en.users.fields.name), { target: { value: 'carol' } })
    fireEvent.change(d.getByLabelText(en.users.fields.password), {
      target: { value: 'password123' },
    })
    await d.findByRole('option', { name: 'Sales' })
    fireEvent.change(d.getByLabelText(en.users.fields.group), { target: { value: 'Sales' } })
    fireEvent.change(d.getByLabelText(en.users.fields.email), {
      target: { value: 'carol@example.com' },
    })
    fireEvent.click(d.getByRole('button', { name: en.common.save }))

    await waitFor(() => expect(callsTo(fetchMock, '/api/users', 'POST')).toHaveLength(1))
    const [, init] = callsTo(fetchMock, '/api/users', 'POST')[0]
    expect(JSON.parse(String(init?.body))).toEqual({
      name: 'carol',
      password: 'password123',
      group_name: 'Sales',
      email: 'carol@example.com',
      note: '',
      is_admin: false,
    })
    await waitFor(() =>
      expect(callsTo(fetchMock, '/api/users').length).toBeGreaterThan(listsBefore),
    )
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })

  it('delete button disabled for active user', async () => {
    setupOrg()
    renderApp('/users')
    await screen.findByText('alice')
    const activeDelete = within(rowOf('alice')).getByRole('button', { name: en.common.delete })
    expect(activeDelete).toBeDisabled()
    expect(activeDelete).toHaveAttribute('title', en.users.deleteActiveHint)
    const disabledDelete = within(rowOf('bob')).getByRole('button', { name: en.common.delete })
    expect(disabledDelete).toBeEnabled()
  })
})
