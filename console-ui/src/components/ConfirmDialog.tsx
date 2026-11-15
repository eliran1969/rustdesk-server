import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Modal } from './Modal'

interface ConfirmDialogProps {
  open: boolean
  title: string
  message: string
  /** Optional secondary note under the message. */
  hint?: string
  confirmLabel: string
  danger?: boolean
  onConfirm: () => unknown
  onCancel: () => void
}

/** A yes/no question on top of `Modal`; the confirm button waits for an async `onConfirm`. */
export function ConfirmDialog({
  open,
  title,
  message,
  hint,
  confirmLabel,
  danger = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)

  async function confirm() {
    setBusy(true)
    try {
      await onConfirm()
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal
      open={open}
      title={title}
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className={danger ? 'btn btn-danger' : 'btn btn-primary'}
            onClick={() => void confirm()}
            disabled={busy}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="confirm-message">{message}</p>
      {hint && <p className="muted small confirm-hint">{hint}</p>}
    </Modal>
  )
}
