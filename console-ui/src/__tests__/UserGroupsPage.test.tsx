import { act, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { renderApp } from './helpers'
import { callsTo, setupOrg } from './orgFixtures'

describe('UserGroupsPage', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'f'.repeat(64))
  })

  it('default group has no delete button', async () => {
    setupOrg()
    renderApp('/user-groups')
    const salesRow = (await screen.findByText('Sales')).closest('tr') as HTMLElement
    const defaultRow = screen.getAllByText('Default')[0].closest('tr') as HTMLElement
    expect(defaultRow).not.toBeNull()
    expect(defaultRow).not.toBe(salesRow)
    const def = within(defaultRow)
    expect(def.getByText(en.userGroups.defaultBadge, { selector: '.badge' })).toBeInTheDocument()
    expect(def.queryByRole('button', { name: en.common.delete })).not.toBeInTheDocument()
    expect(def.getByRole('button', { name: en.common.edit })).toBeInTheDocument()
    expect(within(salesRow).getByRole('button', { name: en.common.delete })).toBeEnabled()
    expect(within(salesRow).getByText('Servers')).toBeInTheDocument()
  })

  it('failed delete keeps the translated error after the list reloads', async () => {
    const fetchMock = setupOrg(undefined, (path, method) =>
      path === '/api/user-groups/g1' && method === 'DELETE'
        ? { status: 400, body: { error: 'Group is not empty' } }
        : undefined,
    )
    renderApp('/user-groups')
    const salesRow = (await screen.findByText('Sales')).closest('tr') as HTMLElement
    const listsBefore = callsTo(fetchMock, '/api/user-groups').length
    fireEvent.click(within(salesRow).getByRole('button', { name: en.common.delete }))
    const dialog = await screen.findByRole('dialog', { name: en.userGroups.confirmDeleteTitle })
    fireEvent.click(within(dialog).getByRole('button', { name: en.common.delete }))

    await waitFor(() =>
      expect(callsTo(fetchMock, '/api/user-groups').length).toBeGreaterThan(listsBefore),
    )
    await act(async () => {
      await new Promise((r) => setTimeout(r, 30))
    })
    expect(screen.getByRole('alert')).toHaveTextContent(en.errors.groupNotEmpty)
  })
})
