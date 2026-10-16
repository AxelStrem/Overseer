import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'

// A field inside a coloured div is not painted itself: the div's tint shows through it. Its
// hover highlight was once the document's background or the card surface, opaque, and covered
// the tint of the row the pointer was on. It is a translucent tint over whatever the field sits
// on. jsdom applies no `:hover`, so the rules are read from the stylesheet.

const styles = readFileSync('src/styles.css', 'utf8').replace(/\/\*[\s\S]*?\*\//g, '')

function rule(selector) {
  const at = styles.search(new RegExp(`(^|\\n|\\})\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{`))
  expect(at, selector).toBeGreaterThanOrEqual(0)
  const open = styles.indexOf('{', at + 1)
  return styles.slice(open + 1, styles.indexOf('}', open))
}

function background(body) {
  return body.match(/background(?:-color)?\s*:\s*([^;]+);/)?.[1].trim()
}

function hoverTints() {
  return [...styles.matchAll(/--hover-tint\s*:\s*([^;]+);/g)].map(m => m[1].trim())
}

describe('a hovered field keeps the tint of the row it is in', () => {
  for (const selector of ['.overseer-field:hover', '.field-value:hover']) {
    it(`${selector} is shaded with a tint, not an opaque theme colour`, () => {
      expect(background(rule(selector))).toBe('var(--hover-tint)')
    })
  }

  it('the tint is translucent in the light theme and the dark one', () => {
    const tints = hoverTints()
    expect(tints).toHaveLength(2)
    for (const tint of tints) {
      const alpha = Number(tint.match(/^rgba\([^)]*,\s*([\d.]+)\s*\)$/)?.[1])
      expect(alpha, tint).toBeGreaterThan(0)
      expect(alpha, tint).toBeLessThan(0.2)
    }
  })
})
