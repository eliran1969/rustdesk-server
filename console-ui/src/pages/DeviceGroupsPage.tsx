import { useCallback, useEffect, useId, useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch, describeError, listQuery } from '../api'
import type { Device, DeviceGroup, ErrorInfo, ListResponse, Rule } from '../api'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { ErrorAlert } from '../components/ErrorAlert'
import { CloseIcon, PlusIcon } from '../components/Icons'
import { Modal } from '../components/Modal'
import { Pager } from '../components/Pager'
import { RuleTags, RulesEditor } from '../components/RulesEditor'
import { wrapPattern } from '../devices'

const PAGE_SIZE = 20
/** Devices listed in the "manage devices" dialog (the API maximum). */
const MEMBERS_PAGE_SIZE = 100
/** Who may connect to devices in this group: user groups and users. */
const INCOMING_TYPES = [0, 2]

/** Device IDs typed by the admin, separated by commas and/or whitespace. */
function parseDeviceIds(text: string): string[] {
  const ids = text.split(/[\s,]+/).filter(Boolean)
  return Array.from(new Set(ids))
}

interface GroupFormProps {
  group?: DeviceGroup
  onClose: () => void
  onSaved: () => void
}

function GroupFormModal({ group, onClose, onSaved }: GroupFormProps) {
  const { t } = useTranslation()
  const formId = useId()
  const editing = group !== undefined
  const initialRules = (group?.allowed_incomings ?? []) as Rule[]
  const [name, setName] = useState(group?.name ?? '')
  const [note, setNote] = useState(group?.note ?? '')
  const [incoming, setIncoming] = useState(initialRules)
  const [error, setError] = useState(null as ErrorInfo | null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    if (!name.trim()) {
      setError({ key: 'errors.nameRequired' })
      return
    }
    // Only changed fields on edit.
    const body: Record<string, unknown> = {}
    if (!group || name.trim() !== group.name) body.name = name.trim()
    if (!group || note !== (group.note ?? '')) body.note = note
    if (!group || JSON.stringify(incoming) !== JSON.stringify(initialRules)) {
      body.allowed_incomings = incoming
    }
    if (group && Object.keys(body).length === 0) {
      onSaved()
      return
    }
    setBusy(true)
    setError(null)
    try {
      if (group) {
        await apiFetch(`/api/device-groups/${group.guid}`, {
          method: 'PATCH',
          body: JSON.stringify(body),
        })
      } else {
        await apiFetch('/api/device-groups', { method: 'POST', body: JSON.stringify(body) })
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
      title={editing ? t('deviceGroups.editTitle') : t('deviceGroups.createTitle')}
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
          <span>{t('deviceGroups.fields.name')}</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            autoComplete="off"
            dir="auto"
          />
        </label>
        <label className="field">
          <span>{t('deviceGroups.fields.note')}</span>
          <textarea value={note} onChange={(e) => setNote(e.target.value)} rows={2} dir="auto" />
        </label>
        <fieldset className="fieldset">
          <legend>{t('deviceGroups.fields.accessibleFrom')}</legend>
          <p className="muted small">{t('deviceGroups.accessibleFromHint')}</p>
          <RulesEditor value={incoming} onChange={setIncoming} allowedTypes={INCOMING_TYPES} />
        </fieldset>
      </form>
    </Modal>
  )
}

interface MembersModalProps {
  group: DeviceGroup
  onClose: () => void
  /** Called after any successful add/remove, so the group list can refresh its counts. */
  onChanged: () => void
}

/** Lists the group's devices (exact `device_group_name` filter), adds by ID and removes. */
function MembersModal({ group, onClose, onChanged }: MembersModalProps) {
  const { t } = useTranslation()
  const inputId = useId()
  const [devices, setDevices] = useState([] as Device[])
  const [total, setTotal] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [ids, setIds] = useState('')
  const [error, setError] = useState(null as ErrorInfo | null)
  const [busy, setBusy] = useState(false)
  const [tick, setTick] = useState(0)

  useEffect(() => {
    let alive = true
    apiFetch(
      listQuery('/api/devices', 1, MEMBERS_PAGE_SIZE, { device_group_name: group.name }),
    )
      .then((res) => {
        if (!alive) return
        const list = res as ListResponse<Device>
        setDevices(list.data ?? [])
        setTotal(list.total ?? 0)
      })
      .catch((err) => {
        if (alive) setError(describeError(err))
      })
      .finally(() => {
        if (alive) setLoaded(true)
      })
    return () => {
      alive = false
    }
  }, [group.name, tick])

  async function change(method: 'POST' | 'DELETE', list: string[]) {
    setBusy(true)
    setError(null)
    const path =
      method === 'POST' ? `/api/device-groups/${group.guid}` : `/api/device-groups/${group.guid}/devices`
    try {
      await apiFetch(path, { method, body: JSON.stringify(list) })
      if (method === 'POST') setIds('')
      onChanged()
      setTick((n) => n + 1)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  function onAdd(e: FormEvent) {
    e.preventDefault()
    const list = parseDeviceIds(ids)
    if (list.length === 0) return
    void change('POST', list)
  }

  return (
    <Modal
      open
      title={t('deviceGroups.manageTitle', { name: group.name })}
      onClose={onClose}
      footer={
        <button type="button" className="btn btn-ghost" onClick={onClose}>
          {t('common.close')}
        </button>
      }
    >
      <div className="form">
        <ErrorAlert error={error} onDismiss={() => setError(null)} />
        <form className="field" onSubmit={onAdd}>
          <label htmlFor={inputId}>{t('deviceGroups.addIds')}</label>
          <span className="input-with-action">
            <input
              id={inputId}
              value={ids}
              onChange={(e) => setIds(e.target.value)}
              aria-describedby={`${inputId}-hint`}
              autoComplete="off"
              dir="ltr"
            />
            <button
              type="submit"
              className="btn btn-primary btn-sm"
              disabled={busy || parseDeviceIds(ids).length === 0}
            >
              <PlusIcon />
              <span>{t('deviceGroups.add')}</span>
            </button>
          </span>
          <small id={`${inputId}-hint`} className="muted field-hint">
            {t('deviceGroups.addIdsHint')}
          </small>
        </form>
        {!loaded && <p className="empty">{t('app.loading')}</p>}
        {loaded && devices.length === 0 && <p className="empty">{t('deviceGroups.noDevices')}</p>}
        {devices.length > 0 && (
          <ul className="member-list">
            {devices.map((d) => (
              <li key={d.guid || d.id}>
                <span className="mono" dir="ltr">
                  {d.id}
                </span>
                <span className="muted" dir="auto">
                  {d.info?.device_name || ''}
                </span>
                <button
                  type="button"
                  className="btn btn-icon btn-plain"
                  onClick={() => void change('DELETE', [d.id])}
                  disabled={busy}
                  aria-label={t('deviceGroups.removeDevice', { id: d.id })}
                  title={t('deviceGroups.removeDevice', { id: d.id })}
                >
                  <CloseIcon />
                </button>
              </li>
            ))}
          </ul>
        )}
        {total > devices.length && (
          <p className="muted small">
            {t('deviceGroups.partialList', { shown: devices.length, total })}
          </p>
        )}
      </div>
    </Modal>
  )
}

/** `null` = closed; `group` absent = create. */
interface Editing {
  group?: DeviceGroup
}

export function DeviceGroupsPage() {
  const { t } = useTranslation()
  const [term, setTerm] = useState('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [groups, setGroups] = useState([] as DeviceGroup[])
  const [total, setTotal] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState(null as ErrorInfo | null)
  /** Failure of the last delete; list reloads never clear it. */
  const [actionError, setActionError] = useState(null as ErrorInfo | null)
  const [editing, setEditing] = useState(null as Editing | null)
  const [managing, setManaging] = useState(null as DeviceGroup | null)
  const [deleting, setDeleting] = useState(null as DeviceGroup | null)
  const [reloadTick, setReloadTick] = useState(0)

  const load = useCallback(
    async function load(isAlive: () => boolean) {
      try {
        const trimmed = query.trim()
        const res = (await apiFetch(
          listQuery('/api/device-groups', page, PAGE_SIZE, {
            name: trimmed ? wrapPattern(trimmed) : '',
          }),
        )) as ListResponse<DeviceGroup>
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

  function startEdit(next: Editing) {
    setActionError(null)
    setEditing(next)
  }

  function startManage(group: DeviceGroup) {
    setActionError(null)
    setManaging(group)
  }

  function startDelete(group: DeviceGroup) {
    setActionError(null)
    setDeleting(group)
  }

  function onSaved() {
    setEditing(null)
    reload()
  }

  async function confirmDelete() {
    if (!deleting) return
    setActionError(null)
    try {
      await apiFetch(`/api/device-groups/${deleting.guid}`, { method: 'DELETE' })
    } catch (err) {
      setActionError(describeError(err))
    }
    setDeleting(null)
    reload()
  }

  const deleteCount = deleting?.device_count ?? 0

  return (
    <section className="page">
      <div className="page-head">
        <h1>{t('deviceGroups.title')}</h1>
        <button type="button" className="btn btn-primary" onClick={() => startEdit({})}>
          <PlusIcon />
          <span>{t('deviceGroups.new')}</span>
        </button>
      </div>

      <form className="toolbar" onSubmit={onSearch} role="search">
        <input
          type="search"
          aria-label={t('deviceGroups.searchPlaceholder')}
          placeholder={t('deviceGroups.searchPlaceholder')}
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

      <ErrorAlert error={actionError} onDismiss={() => setActionError(null)} />
      <ErrorAlert error={error} />

      <div className="card table-card">
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                <th>{t('deviceGroups.columns.name')}</th>
                <th>{t('deviceGroups.columns.note')}</th>
                <th>{t('deviceGroups.columns.devices')}</th>
                <th>{t('deviceGroups.columns.accessibleFrom')}</th>
                <th>{t('common.actions')}</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((g) => (
                <tr key={g.guid}>
                  <td dir="auto">{g.name}</td>
                  <td dir="auto" className="cell-note">
                    {g.note || ''}
                  </td>
                  <td className="cell-num">{g.device_count ?? 0}</td>
                  <td>
                    <RuleTags rules={g.allowed_incomings ?? []} />
                  </td>
                  <td>
                    <div className="row-actions">
                      <button
                        type="button"
                        className="btn btn-ghost btn-sm"
                        onClick={() => startEdit({ group: g })}
                      >
                        {t('common.edit')}
                      </button>
                      <button
                        type="button"
                        className="btn btn-ghost btn-sm"
                        onClick={() => startManage(g)}
                      >
                        {t('deviceGroups.manageDevices')}
                      </button>
                      <button
                        type="button"
                        className="btn btn-danger-ghost btn-sm"
                        onClick={() => startDelete(g)}
                      >
                        {t('common.delete')}
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {loaded && groups.length === 0 && !error && (
            <p className="empty">{t('deviceGroups.empty')}</p>
          )}
          {!loaded && <p className="empty">{t('app.loading')}</p>}
        </div>
        <Pager page={page} pageSize={PAGE_SIZE} total={total} onPage={setPage} />
      </div>

      {editing && (
        <GroupFormModal group={editing.group} onClose={() => setEditing(null)} onSaved={onSaved} />
      )}

      {managing && (
        <MembersModal group={managing} onClose={() => setManaging(null)} onChanged={reload} />
      )}

      <ConfirmDialog
        open={deleting !== null}
        title={t('deviceGroups.confirmDeleteTitle')}
        message={
          deleteCount > 0
            ? t('deviceGroups.confirmDeleteWithDevices', {
                name: deleting?.name ?? '',
                count: deleteCount,
              })
            : t('deviceGroups.confirmDeleteMessage', { name: deleting?.name ?? '' })
        }
        confirmLabel={t('common.delete')}
        danger
        onConfirm={confirmDelete}
        onCancel={() => setDeleting(null)}
      />
    </section>
  )
}
