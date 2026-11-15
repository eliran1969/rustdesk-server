const base = {
  width: 18,
  height: 18,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  'aria-hidden': true,
}

/** Points toward the inline-start edge; mirrored in RTL by `.icon-dir`. */
export function ChevronStart() {
  return (
    <svg {...base} className="icon icon-dir">
      <polyline points="15 18 9 12 15 6" />
    </svg>
  )
}

/** Points toward the inline-end edge; mirrored in RTL by `.icon-dir`. */
export function ChevronEnd() {
  return (
    <svg {...base} className="icon icon-dir">
      <polyline points="9 18 15 12 9 6" />
    </svg>
  )
}

export function MonitorIcon() {
  return (
    <svg {...base} className="icon">
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <line x1="8" y1="21" x2="16" y2="21" />
      <line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  )
}

export function RefreshIcon() {
  return (
    <svg {...base} className="icon">
      <polyline points="23 4 23 10 17 10" />
      <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
    </svg>
  )
}

export function LogoutIcon() {
  return (
    <svg {...base} className="icon icon-dir">
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
      <polyline points="16 17 21 12 16 7" />
      <line x1="21" y1="12" x2="9" y2="12" />
    </svg>
  )
}
