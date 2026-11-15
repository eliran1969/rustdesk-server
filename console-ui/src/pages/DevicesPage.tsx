import { useCallback, useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch, errorKey } from '../api'
import type { Device, DevicesPage as DevicesResponse } from '../api'
import { ChevronEnd, ChevronStart, RefreshIcon } from '../components/Icons'
import { devicesQuery, formatAbsolute, relativeTime } from '../devices'
import type { SearchField } from '../devices'
import { normalizeLanguage } from '../i18n'

const PAGE_SIZE = 20
const REFRESH_MS = 15_000

interface Query {
  field: SearchField
  term: string
}

export function DevicesPage() {
  const { t, i18n } = useTranslation()
  const lang = normalizeLanguage(i18n.language)
  const [field, setField] = useState('id' as SearchField)
  const [term, setTerm] = useState('')
  const [query, setQuery] = useState({ field: 'id', term: '' } as Query)
  const [page, setPage] = useState(1)
  const [devices, setDevices] = useState([] as Device[])
  const [total, setTotal] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState('')
  const [now, setNow] = useState(() => new Date())

  const load = useCallback(
    async function load(isAlive: () => boolean) {
      try {
        const res = (await apiFetch(
          devicesQuery(page, PAGE_SIZE, query.field, query.term),
        )) as DevicesResponse
        if (!isAlive()) return
        const count = res.total ?? 0
        const lastPage = Math.max(1, Math.ceil(count / PAGE_SIZE))
        if (page > lastPage) {
          // Devices disappeared since the page was chosen: move to the last page.
          setPage(lastPage)
          return
        }
        setDevices(res.data ?? [])
        setTotal(count)
        setError('')
      } catch (err) {
        if (isAlive()) setError(errorKey(err))
      } finally {
        if (isAlive()) {
          setLoaded(true)
          setNow(new Date())
        }
      }
    },
    [page, query],
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
  }, [load])

  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE))

  function onSearch(e: FormEvent) {
    e.preventDefault()
    setPage(1)
    setQuery({ field, term })
  }

  function onClear() {
    setTerm('')
    setPage(1)
    setQuery({ field, term: '' })
  }

  function onRefresh() {
    void load(() => true)
  }

  function prevPage() {
    setPage(Math.max(1, page - 1))
  }

  function nextPage() {
    setPage(Math.min(pages, page + 1))
  }

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
        <button type="submit" className="btn btn-primary">
          {t('devices.search')}
        </button>
        {query.term && (
          <button type="button" className="btn btn-ghost" onClick={onClear}>
            {t('devices.clear')}
          </button>
        )}
      </form>

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
                <th>{t('devices.columns.id')}</th>
                <th>{t('devices.columns.deviceName')}</th>
                <th>{t('devices.columns.osUser')}</th>
                <th>{t('devices.columns.os')}</th>
                <th>{t('devices.columns.version')}</th>
                <th>{t('devices.columns.user')}</th>
                <th>{t('devices.columns.online')}</th>
                <th>{t('devices.columns.lastSeen')}</th>
                <th>{t('devices.columns.note')}</th>
              </tr>
            </thead>
            <tbody>
              {devices.map((d) => (
                <tr key={d.guid || d.id}>
                  <td>
                    <span className="mono" dir="ltr">
                      {d.id}
                    </span>
                    {d.status === 0 && <span className="badge">{t('devices.disabled')}</span>}
                  </td>
                  <td dir="auto">{d.info?.device_name || '—'}</td>
                  <td dir="auto">{d.info?.username || '—'}</td>
                  <td dir="auto">{d.info?.os || '—'}</td>
                  <td dir="ltr" className="cell-num">
                    {d.info?.version || '—'}
                  </td>
                  <td dir="auto">{d.user_name || '—'}</td>
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
                </tr>
              ))}
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
              onClick={prevPage}
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
              onClick={nextPage}
              disabled={page >= pages}
              aria-label={t('devices.next')}
              title={t('devices.next')}
            >
              <ChevronEnd />
            </button>
          </div>
        </div>
      </div>
    </section>
  )
}
