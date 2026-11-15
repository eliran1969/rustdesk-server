import { useTranslation } from 'react-i18next'
import { ChevronEnd, ChevronStart } from './Icons'

interface PagerProps {
  page: number
  pageSize: number
  total: number
  onPage: (page: number) => void
}

export function Pager({ page, pageSize, total, onPage }: PagerProps) {
  const { t } = useTranslation()
  const pages = Math.max(1, Math.ceil(total / pageSize))
  return (
    <div className="pager">
      <span className="muted">{t('common.total', { count: total })}</span>
      <div className="pager-controls">
        <button
          type="button"
          className="btn btn-icon"
          onClick={() => onPage(Math.max(1, page - 1))}
          disabled={page <= 1}
          aria-label={t('common.prev')}
          title={t('common.prev')}
        >
          <ChevronStart />
        </button>
        <span>{t('common.pageOf', { page, pages })}</span>
        <button
          type="button"
          className="btn btn-icon"
          onClick={() => onPage(Math.min(pages, page + 1))}
          disabled={page >= pages}
          aria-label={t('common.next')}
          title={t('common.next')}
        >
          <ChevronEnd />
        </button>
      </div>
    </div>
  )
}
