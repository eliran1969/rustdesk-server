import { useCallback, useEffect, useId, useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch, describeError } from '../api'
import type { Device, DevicesPage as DevicesResponse, ErrorInfo, ListResponse } from '../api'
import { useAuth } from '../auth'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { DeviceEditDrawer } from '../components/DeviceEditDrawer'
import { ErrorAlert } from '../components/ErrorAlert'
import { ChevronEnd, ChevronStart, CloseIcon, RefreshIcon } from '../components/Icons'
import { Modal } from '../components/Modal'
import { devicesQuery, formatAbsolute, relativeTime } from '../devices'
import type { SearchField } from '../devices'
import { normalizeLanguage } from '../i18n'

const PAGE_SIZE = 20
const REFRESH_MS = 15_000

interface Query {
  field: SearchField
  term: string
  /** Exact device group name; empty = all. */
  deviceGroup: string
}

type AssignKind = 'user' | 'group'

/** One request of a batch action, for one device. */
interface DeviceAction {
  (guid: string): Promise<unknown>
}

interface BatchResult {
  ok: number
  failed: string[]
}

/** Names from a list endpoint (admin pickers); empty when disabled or on failure. */
function useNames(path: string, enabled: boolean): string[] {
  const [names, setNames] = useState([] as string[])
  useEffect(() => {
    if (!enabled) return
    let alive = true
    apiFetch(path)
      .then((res) => {
        if (alive) setNames(((res as ListResponse<{ name: string }>).data ?? []).map((x) => x.name))
      })
      .catch(() => {
        if (alive) setNames([])
      })
    return () => {
      alive = false
    }
  }, [path, enabled])
  return names
}

interface AssignModalProps {
  kind: AssignKind
  count: number
  options: string[]
  onCancel: () => void
  onApply: (value: string) => void
}

/** Picks the user / device group for a batch assignment; "none" (`""`) unassigns. */
function AssignModal({ kind, count, options, onCancel, onApply }: AssignModalProps) {
  const { t } = useTranslation()
  const selectId = useId()
  const [value, setValue] = useState('')
  return (
    <Modal
      open
      title={
        kind === 'user'
          ? t('devices.batch.assignUserTitle', { count })
          : t('devices.batch.assignGroupTitle', { count })
      }
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            {t('common.cancel')}
          </button>
          <button type="button" className="btn btn-primary" onClick={() => onApply(value)}>
            {t('devices.batch.apply')}
          </button>
        </>
      }
    >
      <div className="form">
        <label className="field" htmlFor={selectId}>
          <span>{kind === 'user' ? t('devices.edit.user') : t('devices.edit.deviceGroup')}</span>
          <select id={selectId} value={value} onChange={(e) => setValue(e.target.value)}>
            <option value="">
              {kind === 'user' ? t('devices.noUser') : t('devices.noDeviceGroup')}
            </option>
            {options.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </select>
        </label>
      </div>
    </Modal>
  )
}

export function DevicesPage() {
  const { t, i18n } = useTranslation()
  const lang = normalizeLanguage(i18n.language)
  const { user: me } = useAuth()
  const isAdmin = me?.is_admin === true
  const userNames = useNames('/api/users?pageSize=100', isAdmin)
  const deviceGroupNames = useNames('/api/device-groups?pageSize=100', isAdmin)
  const [field, setField] = useState('id' as SearchField)
  const [term, setTerm] = useState('')
  const [deviceGroup, setDeviceGroup] = useState('')
  const [query, setQuery] = useState({ field: 'id', term: '', deviceGroup: '' } as Query)
  const [page, setPage] = useState(1)
  const [devices, setDevices] = useState([] as Device[])
  const [total, setTotal] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState('')
  const [now, setNow] = useState(() => new Date())
  const [reloadTick, setReloadTick] = useState(0)
  /** Selected devices on the current page/filter: guid → device ID. */
  const [selected, setSelected] = useState({} as Record<string, string>)
  const [assigning, setAssigning] = useState(null as AssignKind | null)
  const [progress, setProgress] = useState(null as { done: number; total: number } | null)
  /** Outcome of the last batch action; list reloads never clear it. */
  const [batchResult, setBatchResult] = useState(null as BatchResult | null)
  /** Failure of the last row action; list reloads never clear it. */
  const [actionError, setActionError] = useState(null as ErrorInfo | null)
  const [editing, setEditing] = useState(null as Device | null)
  const [deleting, setDeleting] = useState(null as Device | null)

  /** Page and filter changes start a fresh selection. */
  const goToPage = useCallback(function goToPage(next: number) {
    setSelected({})
    setPage(next)
  }, [])

  function changeQuery(next: Query) {
    setSelected({})
    setPage(1)
    setQuery(next)
  }

  const load = useCallback(
    async function load(isAlive: () => boolean) {
      try {
        const res = (await apiFetch(
          devicesQuery(page, PAGE_SIZE, query.field, query.term, {
            device_group_name: query.deviceGroup,
          }),
        )) as DevicesResponse
        if (!isAlive()) return
        const count = res.total ?? 0
        const lastPage = Math.max(1, Math.ceil(count / PAGE_SIZE))
        if (page > lastPage) {
          // Devices disappeared since the page was chosen: move to the last page.
          goToPage(lastPage)
          return
        }
        setDevices(res.data ?? [])
        setTotal(count)
        setError('')
      } catch (err) {
        if (isAlive()) setError(describeError(err).key)
      } finally {
        if (isAlive()) {
          setLoaded(true)
          setNow(new Date())
        }
      }
    },
    [page, query, goToPage],
  )

  useEffect(() => {
    let alive = true
    const isAlive = () => alive
    void load(isAlive)
    const timer = window.setInterval(() => void load(isAlive), REFRESH_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [load, reloadTick])

  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const selectedCount = Object.keys(selected).length
  const allOnPage = devices.length > 0 && devices.every((d) => d.guid in selected)
  const running = progress !== null

  function reload() {
    setReloadTick((n) => n + 1)
  }

  function onSearch(e: FormEvent) {
    e.preventDefault()
    changeQuery({ field, term, deviceGroup })
  }

  function onDeviceGroupFilter(value: string) {
    setDeviceGroup(value)
    changeQuery({ field, term, deviceGroup: value })
  }

  function onClear() {
    setTerm('')
    setDeviceGroup('')
    changeQuery({ field, term: '', deviceGroup: '' })
  }

  function onRefresh() {
    void load(() => true)
  }

  function toggle(d: Device) {
    setSelected((prev) => {
      const next = { ...prev }
      if (d.guid in next) delete next[d.guid]
      else next[d.guid] = d.id
      return next
    })
  }

  function toggleAll() {
    setSelected(() => {
      if (allOnPage) return {}
      const next: Record<string, string> = {}
      for (const d of devices) next[d.guid] = d.id
      return next
    })
  }

  /** One request per selected device, one at a time; failed devices stay selected. */
  async function runBatch(action: DeviceAction) {
    const entries = Object.entries(selected)
    if (entries.length === 0) return
    setBatchResult(null)
    setActionError(null)
    setProgress({ done: 0, total: entries.length })
    const failed: Record<string, string> = {}
    let ok = 0
    for (let i = 0; i < entries.length; i++) {
      const [guid, id] = entries[i]
      try {
        await action(guid)
        ok++
      } catch {
        failed[guid] = id
      }
      setProgress({ done: i + 1, total: entries.length })
    }
    setProgress(null)
    setBatchResult({ ok, failed: Object.values(failed) })
    setSelected(failed)
    reload()
  }

  function batchStatus(kind: 'enable' | 'disable') {
    void runBatch((guid) => apiFetch(`/api/devices/${guid}/${kind}`, { method: 'POST' }))
  }

  function applyAssign(value: string) {
    const kind = assigning
    setAssigning(null)
    const key = kind === 'user' ? 'user_name' : 'device_group_name'
    void runBatch((guid) =>
      apiFetch(`/api/devices/${guid}`, {
        method: 'PATCH',
        body: JSON.stringify({ [key]: value }),
      }),
    )
  }

  async function setStatus(d: Device, kind: 'enable' | 'disable') {
    setActionError(null)
    try {
      await apiFetch(`/api/devices/${d.guid}/${kind}`, { method: 'POST' })
    } catch (err) {
      setActionError(describeError(err))
    }
    reload()
  }

  function startEdit(d: Device) {
    setActionError(null)
    setEditing(d)
  }

  function startDelete(d: Device) {
    setActionError(null)
    setDeleting(d)
  }

  function onSaved() {
    setEditing(null)
    reload()
  }

  async function confirmDelete() {
    if (!deleting) return
    const guid = deleting.guid
    setActionError(null)
    try {
      await apiFetch(`/api/devices/${guid}`, { method: 'DELETE' })
      setSelected((prev) => {
        const next = { ...prev }
        delete next[guid]
        return next
      })
    } catch (err) {
      setActionError(describeError(err))
    }
    setDeleting(null)
    reload()
  }

  const filtered = query.term || query.deviceGroup

  return (
    <section className="page">
      <div className="page-head">
        <div>
          <h1>{t('devices.title')}</h1>
          <p className="muted small">{t('devices.autoRefresh')}</p>
        </div>
        <button type="button" className="btn btn-ghost" onClick={onRefresh}>
          <RefreshIcon />
          <span>{t('devices.refresh')}</span>
        </button>
      </div>

      <form className="toolbar" onSubmit={onSearch} role="search">
        <select
          aria-label={t('devices.searchBy')}
          value={field}
          onChange={(e) => setField(e.target.value as SearchField)}
        >
          <option value="id">{t('devices.byId')}</option>
          <option value="device_name">{t('devices.byName')}</option>
        </select>
        <input
          type="search"
          aria-label={t('devices.searchPlaceholder')}
          placeholder={t('devices.searchPlaceholder')}
          value={term}
          onChange={(e) => setTerm(e.target.value)}
        />
        {isAdmin && (
          <select
            aria-label={t('devices.filterDeviceGroup')}
            value={deviceGroup}
            onChange={(e) => onDeviceGroupFilter(e.target.value)}
          >
            <option value="">{t('devices.allDeviceGroups')}</option>
            {deviceGroupNames.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        )}
        <button type="submit" className="btn btn-primary">
          {t('devices.search')}
        </button>
        {filtered && (
          <button type="button" className="btn btn-ghost" onClick={onClear}>
            {t('devices.clear')}
          </button>
        )}
      </form>

      {isAdmin && (selectedCount > 0 || running) && (
        <div className="batch-bar card" role="toolbar" aria-label={t('devices.batch.label')}>
          <span className="batch-count">{t('devices.selected', { count: selectedCount })}</span>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => setAssigning('user')}
            disabled={running}
          >
            {t('devices.batch.assignUser')}
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => setAssigning('group')}
            disabled={running}
          >
            {t('devices.batch.assignGroup')}
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => batchStatus('enable')}
            disabled={running}
          >
            {t('devices.batch.enable')}
          </button>
          <button
            type="button"
            className="btn btn-danger-ghost btn-sm"
            onClick={() => batchStatus('disable')}
            disabled={running}
            title={t('devices.disableHint')}
          >
            {t('devices.batch.disable')}
          </button>
          <button
            type="button"
            className="btn btn-plain btn-sm"
            onClick={() => setSelected({})}
            disabled={running}
          >
            {t('devices.batch.clearSelection')}
          </button>
          {progress && (
            <span className="muted small" role="status">
              {t('devices.batch.progress', progress)}
            </span>
          )}
        </div>
      )}

      {batchResult && (
        <div
          className={
            batchResult.failed.length > 0
              ? 'notice notice-dismissible'
              : 'notice notice-success notice-dismissible'
          }
          role="status"
        >
          <div className="alert-text">
            <div>
              {t('devices.batch.summary', {
                ok: batchResult.ok,
                failed: batchResult.failed.length,
              })}
            </div>
            {batchResult.failed.length > 0 && (
              <div className="alert-detail" dir="ltr">
                {t('devices.batch.failedIds', { ids: batchResult.failed.join(', ') })}
              </div>
            )}
          </div>
          <button
            type="button"
            className="alert-close"
            onClick={() => setBatchResult(null)}
            aria-label={t('common.dismiss')}
            title={t('common.dismiss')}
          >
            <CloseIcon />
          </button>
        </div>
      )}

      <ErrorAlert error={actionError} onDismiss={() => setActionError(null)} />
      {error && (
        <div className="alert" role="alert">
          {t(error)}
        </div>
      )}

      <div className="card table-card">
        <div className="table-scroll">
          <table className="table">
            <thead>
              <tr>
                {isAdmin && (
                  <th className="cell-check">
                    <input
                      type="checkbox"
                      checked={allOnPage}
                      onChange={toggleAll}
                      disabled={devices.length === 0 || running}
                      aria-label={t('devices.selectAll')}
                      title={t('devices.selectAll')}
                    />
                  </th>
                )}
                <th>{t('devices.columns.id')}</th>
                <th>{t('devices.columns.deviceName')}</th>
                <th>{t('devices.columns.osUser')}</th>
                <th>{t('devices.columns.os')}</th>
                <th>{t('devices.columns.version')}</th>
                <th>{t('devices.columns.user')}</th>
                <th>{t('devices.columns.userGroup')}</th>
                <th>{t('devices.columns.deviceGroup')}</th>
                <th>{t('devices.columns.online')}</th>
                <th>{t('devices.columns.lastSeen')}</th>
                <th>{t('devices.columns.note')}</th>
                {isAdmin && <th>{t('common.actions')}</th>}
              </tr>
            </thead>
            <tbody>
              {devices.map((d) => {
                const enabled = d.status !== 0
                return (
                  <tr key={d.guid || d.id}>
                    {isAdmin && (
                      <td className="cell-check">
                        <input
                          type="checkbox"
                          checked={d.guid in selected}
                          onChange={() => toggle(d)}
                          disabled={running}
                          aria-label={t('devices.selectRow', { id: d.id })}
                        />
                      </td>
                    )}
                    <td>
                      <span className="mono" dir="ltr">
                        {d.id}
                      </span>
                      {!enabled && <span className="badge">{t('devices.disabled')}</span>}
                    </td>
                    <td dir="auto">{d.info?.device_name || '—'}</td>
                    <td dir="auto">{d.info?.username || '—'}</td>
                    <td dir="auto">{d.info?.os || '—'}</td>
                    <td dir="ltr" className="cell-num">
                      {d.info?.version || '—'}
                    </td>
                    <td dir="auto">{d.user_name || '—'}</td>
                    <td dir="auto">{d.group_name || '—'}</td>
                    <td dir="auto">{d.device_group_name || '—'}</td>
                    <td>
                      <span
                        className={d.is_online ? 'dot dot-on' : 'dot dot-off'}
                        role="img"
                        aria-label={d.is_online ? t('devices.online') : t('devices.offline')}
                        title={d.is_online ? t('devices.online') : t('devices.offline')}
                      />
                    </td>
                    <td title={d.last_online ? formatAbsolute(d.last_online, lang) : undefined}>
                      {(d.last_online && relativeTime(d.last_online, lang, now)) ||
                        t('devices.never')}
                    </td>
                    <td dir="auto" className="cell-note">
                      {d.note || ''}
                    </td>
                    {isAdmin && (
                      <td>
                        <div className="row-actions">
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            onClick={() => startEdit(d)}
                          >
                            {t('common.edit')}
                          </button>
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            onClick={() => void setStatus(d, enabled ? 'disable' : 'enable')}
                            title={enabled ? t('devices.disableHint') : undefined}
                          >
                            {enabled ? t('devices.disable') : t('devices.enable')}
                          </button>
                          <button
                            type="button"
                            className="btn btn-danger-ghost btn-sm"
                            onClick={() => startDelete(d)}
                            disabled={enabled}
                            title={enabled ? t('devices.deleteEnabledHint') : undefined}
                          >
                            {t('common.delete')}
                          </button>
                        </div>
                      </td>
                    )}
                  </tr>
                )
              })}
            </tbody>
          </table>
          {loaded && devices.length === 0 && !error && (
            <p className="empty">{t('devices.empty')}</p>
          )}
          {!loaded && <p className="empty">{t('app.loading')}</p>}
        </div>
        <div className="pager">
          <span className="muted">{t('devices.total', { count: total })}</span>
          <div className="pager-controls">
            <button
              type="button"
              className="btn btn-icon"
              onClick={() => goToPage(Math.max(1, page - 1))}
              disabled={page <= 1}
              aria-label={t('devices.prev')}
              title={t('devices.prev')}
            >
              <ChevronStart />
            </button>
            <span>{t('devices.pageOf', { page, pages })}</span>
            <button
              type="button"
              className="btn btn-icon"
              onClick={() => goToPage(Math.min(pages, page + 1))}
              disabled={page >= pages}
              aria-label={t('devices.next')}
              title={t('devices.next')}
            >
              <ChevronEnd />
            </button>
          </div>
        </div>
      </div>

      {assigning && (
        <AssignModal
          kind={assigning}
          count={selectedCount}
          options={assigning === 'user' ? userNames : deviceGroupNames}
          onCancel={() => setAssigning(null)}
          onApply={applyAssign}
        />
      )}

      {editing && (
        <DeviceEditDrawer
          device={editing}
          users={userNames}
          deviceGroups={deviceGroupNames}
          onClose={() => setEditing(null)}
          onSaved={onSaved}
        />
      )}

      <ConfirmDialog
        open={deleting !== null}
        title={t('devices.confirmDeleteTitle')}
        message={t('devices.confirmDeleteMessage', { id: deleting?.id ?? '' })}
        hint={t('devices.deleteOnlineHint')}
        confirmLabel={t('common.delete')}
        danger
        onConfirm={confirmDelete}
        onCancel={() => setDeleting(null)}
      />
    </section>
  )
}
