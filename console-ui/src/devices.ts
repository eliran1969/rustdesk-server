export type SearchField = 'id' | 'device_name'

/** Like res/devices.py: a term without `%` is wrapped as `%term%` (substring match). */
export function wrapPattern(term: string): string {
  return term.includes('%') ? term : `%${term}%`
}

export function devicesQuery(
  page: number,
  pageSize: number,
  field: SearchField,
  term: string,
): string {
  const params = new URLSearchParams({ current: String(page), pageSize: String(pageSize) })
  const trimmed = term.trim()
  if (trimmed) params.set(field, wrapPattern(trimmed))
  return `/api/devices?${params.toString()}`
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

/** `last_online` is `YYYY-MM-DDTHH:MM:SS` in UTC (no zone suffix). */
export function parseUtc(value: string): Date | null {
  const iso = /[zZ]|[+-]\d\d:?\d\d$/.test(value) ? value : `${value.replace(' ', 'T')}Z`
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? null : d
}

/** Relative "last seen" text in the given language, e.g. "5 minutes ago" / "לפני 5 דקות". */
export function relativeTime(value: string, lang: string, now: Date = new Date()): string | null {
  const d = parseUtc(value)
  if (!d) return null
  const diff = Math.round((d.getTime() - now.getTime()) / 1000)
  const fmt = new Intl.RelativeTimeFormat(lang, { numeric: 'auto' })
  for (const [unit, secs] of UNITS) {
    if (Math.abs(diff) >= secs) return fmt.format(Math.trunc(diff / secs), unit)
  }
  return fmt.format(0, 'second')
}

export function formatAbsolute(value: string, lang: string): string {
  const d = parseUtc(value)
  return d ? d.toLocaleString(lang) : value
}
