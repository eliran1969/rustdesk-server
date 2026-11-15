import { useCallback, useEffect, useId, useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch, describeError, isDefaultGroup, listQuery } from '../api'
import type { ErrorInfo, ListResponse, ManagedUser, UserGroup } from '../api'
import { useAuth } from '../auth'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { ErrorAlert } from '../components/ErrorAlert'
import { PlusIcon } from '../components/Icons'
import { Modal } from '../components/Modal'
import { Pager } from '../components/Pager'
import { formatAbsolute, relativeTime, wrapPattern } from '../devices'
import { normalizeLanguage } from '../i18n'

const PAGE_SIZE = 20
const MIN_PASSWORD = 8

type ConfirmKind = 'enable' | 'disable' | 'logout' | 'delete'

interface Pending {
  kind: ConfirmKind
  user: ManagedUser
}

/** `null` = closed; `user` absent = create. */
interface Editing {
  user?: ManagedUser
}

function useGroupNames(): string[] {
  const [names, setNames] = useState([] as string[])
  useEffect(() => {
    let alive = true
    apiFetch('/api/user-groups?pageSize=100')
      .then((res) => {
        if (alive) setNames(((res as ListResponse<UserGroup>).data ?? []).map((g) => g.name))
      })
      .catch(() => {
        if (alive) setNames([])
      })
    return () => {
      alive = false
    }
  }, [])
  return names
}

interface UserFormProps {
  user?: ManagedUser
  groups: string[]
  isSelf: boolean
  onClose: () => void
  onSaved: () => void
}

function UserFormModal({ user, groups, isSelf, onClose, onSaved }: UserFormProps) {
  const { t } = useTranslation()
  const formId = useId()
  const editing = user !== undefined
  const [name, setName] = useState(user?.name ?? '')
  const [displayName, setDisplayName] = useState(user?.display_name ?? '')
  const [password, setPassword] = useState('')
  const initialGroup = user && !isDefaultGroup(user.group_name) ? (user.group_name ?? '') : ''
  const [group, setGroup] = useState(initialGroup)
  const [email, setEmail] = useState(user?.email ?? '')
  const [note, setNote] = useState(user?.note ?? '')
  const [isAdmin, setIsAdmin] = useState(user?.is_admin ?? false)
  const [error, setError] = useState(null as ErrorInfo | null)
  const [busy, setBusy] = useState(false)
  const groupOptions = groups.filter((g) => !isDefaultGroup(g))
  if (group && !groupOptions.includes(group)) groupOptions.unshift(group)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    if (!editing && !name.trim()) {
      setError({ key: 'errors.nameRequired' })
      return
    }
    if ((!editing || password) && password.length < MIN_PASSWORD) {
      setError({ key: 'errors.passwordTooShort' })
      return
    }
    setBusy(true)
    setError(null)
    try {
      if (user) {
        // Only changed fields; `group_name: ""` means Default on the server. Never send
        // `is_admin` for the signed-in user's own account.
        const body: Record<string, unknown> = {}
        if (displayName !== user.display_name) body.display_name = displayName
        if (email !== user.email) body.email = email
        if (note !== user.note) body.note = note
        if (group !== initialGroup) body.group_name = group
        if (!isSelf && isAdmin !== user.is_admin) body.is_admin = isAdmin
        if (password) body.password = password
        if (Object.keys(body).length > 0) {
          await apiFetch(`/api/users/${user.guid}`, {
            method: 'PATCH',
            body: JSON.stringify(body),
          })
        }
      } else {
        await apiFetch('/api/users', {
          method: 'POST',
          body: JSON.stringify({
            name: name.trim(),
            password,
            group_name: group,
            email,
            note,
            is_admin: isAdmin,
          }),
        })
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
      title={editing ? t('users.editTitle') : t('users.createTitle')}
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
      <form id={formId} className="form form-grid" onSubmit={(e) => void onSubmit(e)} noValidate>
        <ErrorAlert error={error} />
        <label className="field">
          <span>{t('users.fields.name')}</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            readOnly={editing}
            required={!editing}
            autoComplete="off"
            dir="auto"
          />
        </label>
        {editing && (
          <label className="field">
            <span>{t('users.fields.displayName')}</span>
            <input value={displayName} onChange={(e) => setDisplayName(e.target.value)} dir="auto" />
          </label>
        )}
        <label className="field">
          <span>{t('users.fields.password')}</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            required={!editing}
            minLength={MIN_PASSWORD}
            aria-describedby={`${formId}-pw-hint`}
          />
        </label>
        <small id={`${formId}-pw-hint`} className="muted field-hint">
          {editing ? t('users.passwordKeepHint') : t('users.passwordHint')}
        </small>
        <label className="field">
          <span>{t('users.fields.group')}</span>
          <select value={group} onChange={(e) => setGroup(e.target.value)}>
            <option value="">{t('users.defaultGroupOption')}</option>
            {groupOptions.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t('users.fields.email')}</span>
          <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} dir="ltr" />
        </label>
        <label className="field">
          <span>{t('users.fields.note')}</span>
          <textarea value={note} onChange={(e) => setNote(e.target.value)} rows={2} dir="auto" />
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={isAdmin}
            onChange={(e) => setIsAdmin(e.target.checked)}
            disabled={isSelf}
          />
          <span>{t('users.fields.isAdmin')}</span>
        </label>
        {isSelf && <small className="muted field-hint">{t('users.selfAdminHint')}</small>}
      </form>
    </Modal>
  )
}

export function UsersPage() {
  const { t, i18n } = useTranslation()
  const lang = normalizeLanguage(i18n.language)
  const { user: me } = useAuth()
  const groups = useGroupNames()
  const [term, setTerm] = useState('')
  const [groupFilter, setGroupFilter] = useState('')
  const [query, setQuery] = useState({ term: '', group: '' })
  const [page, setPage] = useState(1)
  const [users, setUsers] = useState([] as ManagedUser[])
  const [total, setTotal] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState(null as ErrorInfo | null)
  /** Failure of the last row action; list reloads never clear it. */
  const [actionError, setActionError] = useState(null as ErrorInfo | null)
  const [editing, setEditing] = useState(null as Editing | null)
  const [pending, setPending] = useState(null as Pending | null)
  const [reloadTick, setReloadTick] = useState(0)

  const load = useCallback(
    async function load(isAlive: () => boolean) {
      try {
        const filters: Record<string, string> = { group_name: query.group }
        const trimmed = query.term.trim()
        if (trimmed) filters.name = wrapPattern(trimmed)
        const res = (await apiFetch(
          listQuery('/api/users', page, PAGE_SIZE, filters),
        )) as ListResponse<ManagedUser>
        if (!isAlive()) return
        const count = res.total ?? 0
        const lastPage = Math.max(1, Math.ceil(count / PAGE_SIZE))
        if (page > lastPage) {
          setPage(lastPage)
          return
        }
        setUsers(res.data ?? [])
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
    setQuery({ term, group: groupFilter })
  }

  function onGroupFilter(value: string) {
    setGroupFilter(value)
    setPage(1)
    setQuery({ term, group: value })
  }

  function onClear() {
    setTerm('')
    setGroupFilter('')
    setPage(1)
    setQuery({ term: '', group: '' })
  }

  function startEdit(next: Editing) {
    setActionError(null)
    setEditing(next)
  }

  function startAction(next: Pending) {
    setActionError(null)
    setPending(next)
  }

  function onSaved() {
    setEditing(null)
    reload()
  }

  async function runPending() {
    if (!pending) return
    const { kind, user } = pending
    setActionError(null)
    try {
      if (kind === 'enable' || kind === 'disable') {
        await apiFetch(`/api/users/${user.guid}/${kind}`, { method: 'POST' })
      } else if (kind === 'logout') {
        await apiFetch('/api/users/force-logout', {
          method: 'POST',
          body: JSON.stringify({ user_guids: [user.guid] }),
        })
      } else {
        await apiFetch(`/api/users/${user.guid}`, { method: 'DELETE' })
      }
    } catch (err) {
      setActionError(describeError(err))
    }
    setPending(null)
    reload()
  }

  const confirmText: Record<ConfirmKind, { title: string; message: string; label: string }> = {
    enable: {
      title: t('users.confirm.enableTitle'),
      message: t('users.confirm.enableMessage', { name: pending?.user.name ?? '' }),
      label: t('users.enable'),
    },
    disable: {
      title: t('users.confirm.disableTitle'),
      message: t('users.confirm.disableMessage', { name: pending?.user.name ?? '' }),
      label: t('users.disable'),
    },
    logout: {
      title: t('users.confirm.logoutTitle'),
      message: t('users.confirm.logoutMessage', { name: pending?.user.name ?? '' }),
      label: t('users.forceLogout'),
    },
    delete: {
      title: t('users.confirm.deleteTitle'),
      message: t('users.confirm.deleteMessage', { name: pending?.user.name ?? '' }),
      label: t('common.delete'),
    },
  }
  const confirm = pending ? confirmText[pending.kind] : null

  return (
    <section className="page">
      <div className="page-head">
        <h1>{t('users.title')}</h1>
        <button type="button" className="btn btn-primary" onClick={() => startEdit({})}>
          <PlusIcon />
          <span>{t('users.new')}</span>
        </button>
      </div>

      <form className="toolbar" onSubmit={onSearch} role="search">
        <input
          type="search"
          aria-label={t('users.searchPlaceholder')}
          placeholder={t('users.searchPlaceholder')}
          value={term}
          onChange={(e) => setTerm(e.target.value)}
        />
        <select
          aria-label={t('users.filterGroup')}
          value={groupFilter}
          onChange={(e) => onGroupFilter(e.target.value)}
        >
          <option value="">{t('users.allGroups')}</option>
          {groups.map((g) => (
            <option key={g} value={g}>
              {g}
            </option>
          ))}
        </select>
        <button type="submit" className="btn btn-primary">
          {t('common.search')}
        </button>
        {(query.term || query.group) && (
          <button type="button" className="btn btn-ghost" onClick={onClear}>
            {t('common.clear')}
          </button>
        )}
      </form>

      <ErrorAlert error={actionError} onDismiss={() => setActionError(null)} />
      <ErrorAlert error={error} />

      <div className="card table-card">
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th>{t('users.columns.name')}</th>
                <th>{t('users.columns.displayName')}</th>
                <th>{t('users.columns.email')}</th>
                <th>{t('users.columns.group')}</th>
                <th>{t('users.columns.admin')}</th>
                <th>{t('users.columns.status')}</th>
                <th>{t('users.columns.created')}</th>
                <th>{t('common.actions')}</th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => {
                const active = u.status !== 0
                const isSelf = me?.name === u.name
                return (
                  <tr key={u.guid}>
                    <td dir="auto">{u.name}</td>
                    <td dir="auto">{u.display_name || '—'}</td>
                    <td dir="ltr" className="cell-start">
                      {u.email || '—'}
                    </td>
                    <td dir="auto">{u.group_name || '—'}</td>
                    <td>
                      {u.is_admin ? <span className="badge badge-info">{t('users.adminBadge')}</span> : '—'}
                    </td>
                    <td>
                      <span className={active ? 'badge badge-on' : 'badge'}>
                        {active ? t('users.active') : t('users.disabled')}
                      </span>
                    </td>
                    <td title={u.created_at ? formatAbsolute(u.created_at, lang) : undefined}>
                      {(u.created_at && relativeTime(u.created_at, lang)) || '—'}
                    </td>
                    <td>
                      <div className="row-actions">
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm"
                          onClick={() => startEdit({ user: u })}
                        >
                          {t('common.edit')}
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm"
                          onClick={() => startAction({ kind: active ? 'disable' : 'enable', user: u })}
                          disabled={isSelf && active}
                          title={isSelf && active ? t('users.selfHint') : undefined}
                        >
                          {active ? t('users.disable') : t('users.enable')}
                        </button>
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm"
                          onClick={() => startAction({ kind: 'logout', user: u })}
                        >
                          {t('users.forceLogout')}
                        </button>
                        <button
                          type="button"
                          className="btn btn-danger-ghost btn-sm"
                          onClick={() => startAction({ kind: 'delete', user: u })}
                          disabled={active}
                          title={active ? t('users.deleteActiveHint') : undefined}
                        >
                          {t('common.delete')}
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
          {loaded && users.length === 0 && !error && <p className="empty">{t('users.empty')}</p>}
          {!loaded && <p className="empty">{t('app.loading')}</p>}
        </div>
        <Pager page={page} pageSize={PAGE_SIZE} total={total} onPage={setPage} />
      </div>

      {editing && (
        <UserFormModal
          user={editing.user}
          groups={groups}
          isSelf={editing.user !== undefined && editing.user.name === me?.name}
          onClose={() => setEditing(null)}
          onSaved={onSaved}
        />
      )}

      <ConfirmDialog
        open={confirm !== null}
        title={confirm?.title ?? ''}
        message={confirm?.message ?? ''}
        confirmLabel={confirm?.label ?? ''}
        danger={pending?.kind === 'delete' || pending?.kind === 'disable'}
        onConfirm={runPending}
        onCancel={() => setPending(null)}
      />
    </section>
  )
}
