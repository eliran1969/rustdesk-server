import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { describe, expect, it } from 'vitest'

const SRC = join(__dirname, '..')
/** Raw JSX text with letters between a tag end and the next tag (brief regex, verbatim). */
const RAW_JSX_TEXT = />[^<{]*[A-Za-zא-ת][^<{]*</
/**
 * The same rule for text that sits alone on its own line between two tags, e.g.
 *   <p>
 *     Hello
 *   </p>
 */
const RAW_JSX_TEXT_MULTILINE = />[ \t]*\r?\n[ \t]*[^<{\n]*[A-Za-zא-ת][^<{\n]*\r?\n[ \t]*<\//

function tsxFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) {
      return name === '__tests__' ? [] : tsxFiles(full)
    }
    return name.endsWith('.tsx') ? [full] : []
  })
}

/**
 * The regex is applied line by line: over the whole file it would also match
 * TypeScript (arrow functions, generics, statements after a JSX return), which
 * is not JSX text.
 */
function findRawText(source: string): string | null {
  for (const line of source.split(/\r?\n/)) {
    const m = line.match(RAW_JSX_TEXT)
    if (m) return m[0]
  }
  const m = source.match(RAW_JSX_TEXT_MULTILINE)
  return m ? m[0] : null
}

describe('no hardcoded text', () => {
  it('detector catches raw text on one line and across lines', () => {
    expect(findRawText('return <p>Hello</p>')).not.toBeNull()
    expect(findRawText('return <p>שלום</p>')).not.toBeNull()
    expect(findRawText('return (\n  <p>\n    Hello\n  </p>\n)')).not.toBeNull()
    expect(findRawText("return <p>{t('x')}</p>")).toBeNull()
    expect(findRawText('return <p>1 / 2</p>')).toBeNull()
  })

  it('has no raw JSX text with letters in src/**/*.tsx', () => {
    const files = tsxFiles(SRC)
    expect(files.length).toBeGreaterThan(0)
    const offenders = files
      .map((f) => {
        const hit = findRawText(readFileSync(f, 'utf8'))
        return hit ? `${relative(SRC, f)}: ${hit.trim().slice(0, 80)}` : null
      })
      .filter((x) => x !== null)
    expect(offenders).toEqual([])
  })
})
