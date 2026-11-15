import { useId, useState } from 'react'
import type { FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { apiFetch, describeError } from '../api'
import type { Device, ErrorInfo } from '../api'
import { ErrorAlert } from './ErrorAlert'
import { Modal } from './Modal'

interface DeviceEditDrawerProps {
  device: Device
  /** Usernames offered in the "assigned user" picker. */
  users: string[]
  /** Device group names offered in the group picker. */
  deviceGroups: string[]
  onClose: () => void
  onSaved: () => void
}

/** Choices for a picker, keeping the current value even if the list does not contain it. */
function withCurrent(options: string[], current: string): string[] {
  return current && !options.includes(current) ? [current, ...options] : options
}

/**
 * Side drawer for one device. The display name and system user are the effective values
 * (override or reported); an emptied field — or "Reset" — sends `""`, which clears the
 * override. Only changed fields are sent; `""` for user / device group unassigns.
 */
export function DeviceEditDrawer({
  device,
  users,
  deviceGroups,
  onClose,
  onSaved,
}: DeviceEditDrawerProps) {
  const { t } = useTranslation()
  const formId = useId()
  const initial = {
    name: device.info?.device_name ?? '',
    osUser: device.info?.username ?? '',
    note: device.note ?? '',
    user: device.user_name ?? '',
    group: device.device_group_name ?? '',
  }
  const [name, setName] = useState(initial.name)
  const [osUser, setOsUser] = useState(initial.osUser)
  const [note, setNote] = useState(initial.note)
  const [user, setUser] = useState(initial.user)
  const [group, setGroup] = useState(initial.group)
  const [error, setError] = useState(null as ErrorInfo | null)
  const [busy, setBusy] = useState(false)

  async function onSubmit(e: FormEvent) {
    e.preventDefault()
    const body: Record<string, string> = {}
    if (name.trim() !== initial.name) body.device_name = name.trim()
    if (osUser.trim() !== initial.osUser) body.device_username = osUser.trim()
    if (note !== initial.note) body.note = note
    if (user !== initial.user) body.user_name = user
    if (group !== initial.group) body.device_group_name = group
    if (Object.keys(body).length === 0) {
      onSaved()
      return
    }
    setBusy(true)
    setError(null)
    try {
      await apiFetch(`/api/devices/${device.guid}`, {
        method: 'PATCH',
        body: JSON.stringify(body),
      })
      onSaved()
    } catch (err) {
      setError(describeError(err))
      setBusy(false)
    }
  }

  return (
    <Modal
      open
      variant="drawer"
      title={t('devices.edit.title')}
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
        <div className="field">
          <span>{t('devices.edit.id')}</span>
          <span className="mono field-static" dir="ltr">
            {device.id}
          </span>
        </div>
        <div className="field">
          <label htmlFor={`${formId}-name`}>{t('devices.edit.deviceName')}</label>
          <span className="input-with-action">
            <input
              id={`${formId}-name`}
              value={name}
              onChange={(e) => setName(e.target.value)}
              aria-describedby={`${formId}-name-hint`}
              autoComplete="off"
              dir="auto"
            />
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => setName('')}
              disabled={!name}
              aria-label={t('devices.edit.resetField', { field: t('devices.edit.deviceName') })}
              title={t('devices.edit.resetField', { field: t('devices.edit.deviceName') })}
            >
              {t('devices.edit.reset')}
            </button>
          </span>
        </div>
        <small id={`${formId}-name-hint`} className="muted field-hint">
          {t('devices.edit.deviceNameHint')}
        </small>
        <div className="field">
          <label htmlFor={`${formId}-user`}>{t('devices.edit.osUser')}</label>
          <span className="input-with-action">
            <input
              id={`${formId}-user`}
              value={osUser}
              onChange={(e) => setOsUser(e.target.value)}
              aria-describedby={`${formId}-user-hint`}
              autoComplete="off"
              dir="auto"
            />
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => setOsUser('')}
              disabled={!osUser}
              aria-label={t('devices.edit.resetField', { field: t('devices.edit.osUser') })}
              title={t('devices.edit.resetField', { field: t('devices.edit.osUser') })}
            >
              {t('devices.edit.reset')}
            </button>
          </span>
        </div>
        <small id={`${formId}-user-hint`} className="muted field-hint">
          {t('devices.edit.osUserHint')}
        </small>
        <label className="field">
          <span>{t('devices.edit.note')}</span>
          <textarea value={note} onChange={(e) => setNote(e.target.value)} rows={3} dir="auto" />
        </label>
        <label className="field">
          <span>{t('devices.edit.user')}</span>
          <select value={user} onChange={(e) => setUser(e.target.value)}>
            <option value="">{t('devices.noUser')}</option>
            {withCurrent(users, initial.user).map((u) => (
              <option key={u} value={u}>
                {u}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t('devices.edit.deviceGroup')}</span>
          <select value={group} onChange={(e) => setGroup(e.target.value)}>
            <option value="">{t('devices.noDeviceGroup')}</option>
            {withCurrent(deviceGroups, initial.group).map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        </label>
        <div className="device-status-hints muted small" role="note">
          <p>{t('devices.disableHint')}</p>
          <p>{t('devices.deleteOnlineHint')}</p>
        </div>
      </form>
    </Modal>
  )
}
