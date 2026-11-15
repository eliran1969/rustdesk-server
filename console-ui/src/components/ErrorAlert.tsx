import { useTranslation } from 'react-i18next'
import type { ErrorInfo } from '../api'
import { CloseIcon } from './Icons'

/** A translated error; unmapped server text is shown verbatim in a muted detail line. */
export function ErrorAlert({
  error,
  onDismiss,
}: {
  error: ErrorInfo | null
  onDismiss?: () => void
}) {
  const { t } = useTranslation()
  if (!error) return null
  return (
    <div className={onDismiss ? 'alert alert-dismissible' : 'alert'} role="alert">
      <div className="alert-text">
        <div>{t(error.key, error.params ?? {})}</div>
        {error.detail && (
          <div className="alert-detail" dir="auto">
            {error.detail}
          </div>
        )}
      </div>
      {onDismiss && (
        <button
          type="button"
          className="alert-close"
          onClick={onDismiss}
          aria-label={t('common.dismiss')}
          title={t('common.dismiss')}
        >
          <CloseIcon />
        </button>
      )}
    </div>
  )
}
