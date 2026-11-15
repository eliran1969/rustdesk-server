import { useTranslation } from 'react-i18next'
import type { ErrorInfo } from '../api'

/** A translated error; unmapped server text is shown verbatim in a muted detail line. */
export function ErrorAlert({ error }: { error: ErrorInfo | null }) {
  const { t } = useTranslation()
  if (!error) return null
  return (
    <div className="alert" role="alert">
      <div>{t(error.key, error.params ?? {})}</div>
      {error.detail && (
        <div className="alert-detail" dir="auto">
          {error.detail}
        </div>
      )}
    </div>
  )
}
