import { useTranslation } from 'react-i18next'

/** Placeholder until the device groups screen (Phase 2 Task 8) replaces it. */
export function DeviceGroupsPage() {
  const { t } = useTranslation()
  return (
    <section className="page">
      <div className="page-head">
        <h1>{t('deviceGroups.title')}</h1>
      </div>
      <div className="card">
        <p className="empty">{t('deviceGroups.comingSoon')}</p>
      </div>
    </section>
  )
}
