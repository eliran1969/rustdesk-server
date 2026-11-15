import { useCallback, useEffect, useId, useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch, describeError, isDefaultGroup, listQuery } from '../api'
import type { ErrorInfo, ListResponse, Rule, UserGroup } from '../api'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { ErrorAlert } from '../components/ErrorAlert'
import { PlusIcon } from '../components/Icons'
import { Modal } from '../components/Modal'
import { Pager } from '../components/Pager'
import { RuleTags, RulesEditor } from '../components/RulesEditor'
import { wrapPattern } from '../devices'

const PAGE_SIZE = 20
/** Who may connect to this group's members: user groups and users. */
const INCOMING_TYPES = [0, 2]
/** What this group's members may connect to: user groups and device groups. */
const OUTGOING_TYPES = [0, 1]

interface GroupFormProps {
  group?: UserGroup
  onClose: () => void
  onSaved: () => void
}

function GroupFormModal({ group, onClose, onSaved }: GroupFormProps) {
  const { t } = useTranslation()
  const formId = useId()
  const editing = group !== undefined
  const isDefault = editing && isDefaultGroup(group.name)
  const [name, setName] = useState(group?.name ?? '')
  const [note, setNote] = useState(group?.note ?? '')
  const [outgoing, setOutgoing] = useState((group?.allowed_outgoings ?? []) as Rule[])
  const [incoming, setIncoming] = useState((group?.allowed_incomings ?? []) as Rule[])
  const [error, setError] = useState(null as ErrorInfo | null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    if (!isDefault && !name.trim()) {
      setError({ key: 'errors.nameRequired' })
      return
    }
    setBusy(true)
    setError(null)
    const body: Record<string, unknown> = {
      note,
      allowed_incomings: incoming,
      allowed_outgoings: outgoing,
    }
    if (!isDefault) body.name = name.trim()
    try {
      if (group) {
        await apiFetch(`/api/user-groups/${group.guid}`, {
          method: 'PATCH',
          body: JSON.stringify(body),
        })
      } else {
        await apiFetch('/api/user-groups', { method: 'POST', body: JSON.stringify(body) })
      }
      onSaved()
    } catch (err) {
      setError(describeError(err))
      setBusy(false)
    }
  }

  return (
    <Modal
      open
      title={editing ? t('userGroups.editTitle') : t('userGroups.createTitle')}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button type="submit" form={formId} className="btn btn-primary" disabled={busy}>
            {t('common.save')}
          </button>
        </>
      }
    >
      <form id={formId} className="form" onSubmit={(e) => void onSubmit(e)} noValidate>
        <ErrorAlert error={error} />
        <label className="field">
          <span>{t('userGroups.fields.name')}</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            readOnly={isDefault}
            required
            autoComplete="off"
            dir="auto"
          />
        </label>
        {isDefault && <small className="muted field-hint">{t('userGroups.defaultNameHint')}</small>}
        <label className="field">
          <span>{t('userGroups.fields.note')}</span>
          <textarea value={note} onChange={(e) => setNote(e.target.value)} rows={2} dir="auto" />
        </label>
        <fieldset className="fieldset">
          <legend>{t('userGroups.fields.canAccess')}</legend>
          <p className="muted small">{t('userGroups.canAccessHint')}</p>
          <RulesEditor value={outgoing} onChange={setOutgoing} allowedTypes={OUTGOING_TYPES} />
        </fieldset>
        <fieldset className="fieldset">
          <legend>{t('userGroups.fields.accessibleFrom')}</legend>
          <p className="muted small">{t('userGroups.accessibleFromHint')}</p>
          <RulesEditor value={incoming} onChange={setIncoming} allowedTypes={INCOMING_TYPES} />
        </fieldset>
      </form>
    </Modal>
  )
}

/** `null` = closed; `group` absent = create. */
interface Editing {
  group?: UserGroup
}

export function UserGroupsPage() {
  const { t } = useTranslation()
  const [term, setTerm] = useState('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [groups, setGroups] = useState([] as UserGroup[])
  const [total, setTotal] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState(null as ErrorInfo | null)
  const [editing, setEditing] = useState(null as Editing | null)
  const [deleting, setDeleting] = useState(null as UserGroup | null)
  const [reloadTick, setReloadTick] = useState(0)

  const load = useCallback(
    async function load(isAlive: () => boolean) {
      try {
        const trimmed = query.trim()
        const res = (await apiFetch(
          listQuery('/api/user-groups', page, PAGE_SIZE, {
            name: trimmed ? wrapPattern(trimmed) : '',
          }),
        )) as ListResponse<UserGroup>
        if (!isAlive()) return
        const count = res.total ?? 0
        const lastPage = Math.max(1, Math.ceil(count / PAGE_SIZE))
        if (page > lastPage) {
          setPage(lastPage)
          return
        }
        setGroups(res.data ?? [])
        setTotal(count)
        setError(null)
      } catch (err) {
        if (isAlive()) setError(describeError(err))
      } finally {
        if (isAlive()) setLoaded(true)
      }
    },
    [page, query],
  )

  useEffect(() => {
    let alive = true
    void load(() => alive)
    return () => {
      alive = false
    }
  }, [load, reloadTick])

  function reload() {
    setReloadTick((n) => n + 1)
  }

  function onSearch(e: FormEvent) {
    e.preventDefault()
    setPage(1)
    setQuery(term)
  }

  function onClear() {
    setTerm('')
    setPage(1)
    setQuery('')
  }

  function onSaved() {
    setEditing(null)
    reload()
  }

  async function confirmDelete() {
    if (!deleting) return
    try {
      await apiFetch(`/api/user-groups/${deleting.guid}`, { method: 'DELETE' })
      setError(null)
    } catch (err) {
      setError(describeError(err))
    }
    setDeleting(null)
    reload()
  }

  return (
    <section className="page">
      <div className="page-head">
        <h1>{t('userGroups.title')}</h1>
        <button type="button" className="btn btn-primary" onClick={() => setEditing({})}>
          <PlusIcon />
          <span>{t('userGroups.new')}</span>
        </button>
      </div>

      <form className="toolbar" onSubmit={onSearch} role="search">
        <input
          type="search"
          aria-label={t('userGroups.searchPlaceholder')}
          placeholder={t('userGroups.searchPlaceholder')}
          value={term}
          onChange={(e) => setTerm(e.target.value)}
        />
        <button type="submit" className="btn btn-primary">
          {t('common.search')}
        </button>
        {query && (
          <button type="button" className="btn btn-ghost" onClick={onClear}>
            {t('common.clear')}
          </button>
        )}
      </form>

      <ErrorAlert error={error} />

      <div className="card table-card">
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th>{t('userGroups.columns.name')}</th>
                <th>{t('userGroups.columns.note')}</th>
                <th>{t('userGroups.columns.users')}</th>
                <th>{t('userGroups.columns.canAccess')}</th>
                <th>{t('userGroups.columns.accessibleFrom')}</th>
                <th>{t('common.actions')}</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((g) => {
                const isDefault = isDefaultGroup(g.name)
                return (
                  <tr key={g.guid}>
                    <td>
                      <span dir="auto">{g.name}</span>
                      {isDefault && <span className="badge badge-info">{t('userGroups.defaultBadge')}</span>}
                    </td>
                    <td dir="auto" className="cell-note">
                      {g.note || ''}
                    </td>
                    <td className="cell-num">{g.user_count}</td>
                    <td>
                      <RuleTags rules={g.allowed_outgoings ?? []} />
                    </td>
                    <td>
                      <RuleTags rules={g.allowed_incomings ?? []} />
                    </td>
                    <td>
                      <div className="row-actions">
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm"
                          onClick={() => setEditing({ group: g })}
                        >
                          {t('common.edit')}
                        </button>
                        {!isDefault && (
                          <button
                            type="button"
                            className="btn btn-danger-ghost btn-sm"
                            onClick={() => setDeleting(g)}
                          >
                            {t('common.delete')}
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
          {loaded && groups.length === 0 && !error && (
            <p className="empty">{t('userGroups.empty')}</p>
          )}
          {!loaded && <p className="empty">{t('app.loading')}</p>}
        </div>
        <Pager page={page} pageSize={PAGE_SIZE} total={total} onPage={setPage} />
      </div>

      {editing && (
        <GroupFormModal group={editing.group} onClose={() => setEditing(null)} onSaved={onSaved} />
      )}

      <ConfirmDialog
        open={deleting !== null}
        title={t('userGroups.confirmDeleteTitle')}
        message={
          deleting && deleting.user_count > 0
            ? t('userGroups.confirmDeleteNotEmpty', { name: deleting.name, count: deleting.user_count })
            : t('userGroups.confirmDeleteMessage', { name: deleting?.name ?? '' })
        }
        confirmLabel={t('common.delete')}
        danger
        onConfirm={confirmDelete}
        onCancel={() => setDeleting(null)}
      />
    </section>
  )
}
