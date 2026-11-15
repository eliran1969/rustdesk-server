import { describe, expect, it } from 'vitest'
import { ApiError, describeError } from '../api'
import i18n, { setLanguage } from '../i18n'
import en from '../i18n/en.json'
import he from '../i18n/he.json'

function keyPaths(obj: unknown, prefix = ''): string[] {
  if (obj === null || typeof obj !== 'object' || Array.isArray(obj)) {
    return [prefix]
  }
  return Object.entries(obj as Record<string, unknown>)
    .flatMap(([k, v]) => keyPaths(v, prefix ? `${prefix}.${k}` : k))
    .sort()
}

describe('i18n', () => {
  it('maps the phase 2 review error strings to translated keys', () => {
    expect(describeError(new ApiError('Use change password for your own account', 400))).toEqual({
      key: 'errors.selfPasswordChange',
    })
    expect(describeError(new ApiError('User is disabled: bob', 400))).toEqual({
      key: 'errors.userDisabled',
      params: { name: 'bob' },
    })
  })

  it('he and en have identical key sets', () => {
    expect(keyPaths(he)).toEqual(keyPaths(en))
    expect(keyPaths(en).length).toBeGreaterThan(10)
  })

  it('switching to he sets dir=rtl and lang=he', async () => {
    await setLanguage('he')
    expect(document.documentElement.dir).toBe('rtl')
    expect(document.documentElement.lang).toBe('he')
    expect(localStorage.getItem('console_lang')).toBe('he')
    expect(i18n.language).toBe('he')
  })

  it('switching to en sets dir=ltr', async () => {
    await setLanguage('he')
    await setLanguage('en')
    expect(document.documentElement.dir).toBe('ltr')
    expect(document.documentElement.lang).toBe('en')
    expect(localStorage.getItem('console_lang')).toBe('en')
  })
})
