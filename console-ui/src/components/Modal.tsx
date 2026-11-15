import { useEffect, useId, useRef } from 'react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { CloseIcon } from './Icons'

interface ModalProps {
  open: boolean
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll(FOCUSABLE)) as HTMLElement[]
}

/**
 * Accessible dialog: `role="dialog"`, `aria-modal`, labelled by its title. Focus moves
 * into the dialog on open, Tab cycles inside it, Esc closes, and focus returns to the
 * element that opened it.
 */
export function Modal({ open, title, onClose, children, footer }: ModalProps) {
  const { t } = useTranslation()
  const titleId = useId()
  const ref = useRef(null as HTMLDivElement | null)

  const onCloseRef = useRef(onClose)
  useEffect(() => {
    onCloseRef.current = onClose
  }, [onClose])

  useEffect(() => {
    if (!open) return
    const opener = document.activeElement as HTMLElement | null
    const root = ref.current
    if (root) {
      const body = root.querySelector('.modal-body') as HTMLElement | null
      const first = (body && focusables(body)[0]) || focusables(root)[0]
      ;(first ?? root).focus()
    }
    // On `document`, so Esc and the Tab trap still work when focus left the dialog
    // (e.g. after a click on the backdrop).
    function onKeyDown(e: KeyboardEvent) {
      const dialog = ref.current
      if (!dialog) return
      if (e.key === 'Escape') {
        e.stopPropagation()
        onCloseRef.current()
        return
      }
      if (e.key !== 'Tab') return
      const items = focusables(dialog)
      if (items.length === 0) {
        e.preventDefault()
        dialog.focus()
        return
      }
      const first = items[0]
      const last = items[items.length - 1]
      const active = document.activeElement
      const inside = active instanceof Node && dialog.contains(active)
      if (!inside) {
        e.preventDefault()
        ;(e.shiftKey ? last : first).focus()
      } else if (e.shiftKey && (active === first || active === dialog)) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      if (opener && typeof opener.focus === 'function' && opener.isConnected) opener.focus()
    }
  }, [open])

  if (!open) return null

  return (
    <div className="modal-backdrop">
      <div
        ref={ref}
        className="modal card"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <div className="modal-head">
          <h2 id={titleId}>{title}</h2>
          <button
            type="button"
            className="btn btn-icon btn-plain"
            onClick={onClose}
            aria-label={t('common.close')}
            title={t('common.close')}
          >
            <CloseIcon />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  )
}
