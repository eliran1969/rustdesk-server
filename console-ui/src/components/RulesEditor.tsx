import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch } from '../api'
import type { ListResponse, Rule } from '../api'
import { CloseIcon, PlusIcon } from './Icons'

export type { Rule } from '../api'

interface RulesEditorProps {
  value: Rule[]
  onChange: (rules: Rule[]) => void
  /** Rule types offered: 0 user group, 1 device group, 2 user. */
  allowedTypes: number[]
}

/** Where each rule type takes its choices from. */
const SOURCES: Record<number, string> = {
  0: '/api/user-groups?pageSize=100',
  1: '/api/device-groups?pageSize=100',
  2: '/api/users?pageSize=100',
}

const TYPE_KEYS: Record<number, string> = {
  0: 'rules.types.userGroup',
  1: 'rules.types.deviceGroup',
  2: 'rules.types.user',
}

function ruleTypeKey(type: number): string {
  return TYPE_KEYS[type] ?? 'rules.types.unknown'
}

/** Read-only list of rules as tags, each with its type. */
export function RuleTags({ rules }: { rules: Rule[] }) {
  const { t } = useTranslation()
  if (rules.length === 0) return <span className="muted">{'—'}</span>
  return (
    <ul className="tag-list">
      {rules.map((r) => (
        <li key={`${r.type}:${r.name}`} className="tag" title={t(ruleTypeKey(r.type))}>
          <span className="tag-type">{t(ruleTypeKey(r.type))}</span>
          <span dir="auto">{r.name}</span>
        </li>
      ))}
    </ul>
  )
}

/** Picks access-rule targets (groups / users) of the allowed types; shared by group editors. */
export function RulesEditor({ value, onChange, allowedTypes }: RulesEditorProps) {
  const { t } = useTranslation()
  const [type, setType] = useState(allowedTypes[0] ?? 0)
  const [name, setName] = useState('')
  const [options, setOptions] = useState({} as Record<number, string[]>)
  const typesKey = allowedTypes.join(',')

  useEffect(() => {
    let alive = true
    const types = typesKey.split(',').filter(Boolean).map(Number)
    for (const ty of types) {
      const src = SOURCES[ty]
      if (!src) continue
      apiFetch(src)
        .then((res) => {
          const names = ((res as ListResponse<{ name: string }>).data ?? []).map((x) => x.name)
          if (alive) setOptions((prev) => ({ ...prev, [ty]: names }))
        })
        .catch(() => {
          if (alive) setOptions((prev) => ({ ...prev, [ty]: [] }))
        })
    }
    return () => {
      alive = false
    }
  }, [typesKey])

  const chosen = new Set(value.filter((r) => r.type === type).map((r) => r.name))
  const available = (options[type] ?? []).filter((n) => !chosen.has(n))

  function changeType(next: number) {
    setType(next)
    setName('')
  }

  function add() {
    if (!name || !allowedTypes.includes(type)) return
    onChange([...value, { type, name }])
    setName('')
  }

  function remove(rule: Rule) {
    onChange(value.filter((r) => !(r.type === rule.type && r.name === rule.name)))
  }

  return (
    <div className="rules-editor">
      {value.length === 0 ? (
        <p className="muted small">{t('rules.none')}</p>
      ) : (
        <ul className="tag-list">
          {value.map((r) => (
            <li key={`${r.type}:${r.name}`} className="tag">
              <span className="tag-type">{t(ruleTypeKey(r.type))}</span>
              <span dir="auto">{r.name}</span>
              <button
                type="button"
                className="tag-remove"
                onClick={() => remove(r)}
                aria-label={t('rules.remove', { name: r.name })}
                title={t('rules.remove', { name: r.name })}
              >
                <CloseIcon />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="rules-add">
        <select
          aria-label={t('rules.type')}
          value={String(type)}
          onChange={(e) => changeType(Number(e.target.value))}
        >
          {allowedTypes.map((ty) => (
            <option key={ty} value={String(ty)}>
              {t(ruleTypeKey(ty))}
            </option>
          ))}
        </select>
        <select aria-label={t('rules.target')} value={name} onChange={(e) => setName(e.target.value)}>
          <option value="">{t('rules.choose')}</option>
          {available.map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
        <button type="button" className="btn btn-ghost" onClick={add} disabled={!name}>
          <PlusIcon />
          <span>{t('rules.add')}</span>
        </button>
      </div>
    </div>
  )
}
