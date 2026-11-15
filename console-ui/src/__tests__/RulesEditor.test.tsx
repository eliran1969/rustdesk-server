import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { RulesEditor } from '../components/RulesEditor'
import { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import { callsTo, setupOrg } from './orgFixtures'

describe('RulesEditor', () => {
  beforeEach(async () => {
    await setLanguage('en')
    localStorage.setItem('console_token', 'a'.repeat(64))
  })

  it('only allowed types selectable', async () => {
    const fetchMock = setupOrg()
    const onChange = vi.fn()
    render(<RulesEditor value={[]} onChange={onChange} allowedTypes={[0, 2]} />)

    const typeSelect = screen.getByLabelText(en.rules.type) as HTMLSelectElement
    const labels = Array.from(typeSelect.options).map((o) => o.text)
    expect(labels).toEqual([en.rules.types.userGroup, en.rules.types.user])
    expect(labels).not.toContain(en.rules.types.deviceGroup)

    fireEvent.change(typeSelect, { target: { value: '2' } })
    await screen.findByRole('option', { name: 'bob' })
    fireEvent.change(screen.getByLabelText(en.rules.target), { target: { value: 'bob' } })
    fireEvent.click(screen.getByRole('button', { name: en.rules.add }))
    expect(onChange).toHaveBeenCalledWith([{ type: 2, name: 'bob' }])

    await waitFor(() => expect(callsTo(fetchMock, '/api/users').length).toBeGreaterThan(0))
    expect(callsTo(fetchMock, '/api/device-groups')).toHaveLength(0)
  })
})
