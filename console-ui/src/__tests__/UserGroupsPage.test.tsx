import { screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { renderApp } from './helpers'
import { setupOrg } from './orgFixtures'

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
})
