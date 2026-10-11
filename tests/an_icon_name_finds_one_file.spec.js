import { describe, it, expect } from 'vitest'
import path from 'node:path'
import { resolveIcon, collectIcons } from '../src/icons/library.mjs'

// The icon library is one flat name space over several sources. Overseer's own icons come
// first, so the names buttons have always used keep their look; then Lucide; then Tabler, whose
// filled icons take a `-filled` suffix.

const from = (file) => file && file.split(path.sep).join('/')

describe('an icon name finds one file', () => {
  it("finds Overseer's own icons first", () => {
    expect(from(resolveIcon('note'))).toMatch(/src[/]icons[/]note[.]svg$/)
    expect(from(resolveIcon('check'))).toMatch(/src[/]icons[/]check[.]svg$/)
  })

  it('finds Lucide next, and Tabler after it', () => {
    expect(from(resolveIcon('x'))).toMatch(/lucide-static[/]icons[/]x[.]svg$/)
    expect(from(resolveIcon('carrot'))).toMatch(/lucide-static[/]icons[/]carrot[.]svg$/)
    expect(from(resolveIcon('a-b-2'))).toMatch(/tabler[/]icons[/]icons[/]outline[/]a-b-2[.]svg$/)
    expect(from(resolveIcon('heart-filled'))).toMatch(/tabler[/]icons[/]icons[/]filled[/]heart[.]svg$/)
  })

  it('finds nothing for a name it does not have, or one that is not a plain name', () => {
    expect(resolveIcon('no-such-icon-anywhere')).toBeNull()
    expect(resolveIcon('../package')).toBeNull()
    expect(resolveIcon('Note')).toBeNull()
    expect(resolveIcon('')).toBeNull()
  })

  it('gathers every name once, each from the first source to have it', () => {
    const icons = collectIcons()
    expect(icons.size).toBeGreaterThan(5000)
    expect(from(icons.get('cross'))).toMatch(/src[/]icons[/]cross[.]svg$/)
    expect(from(icons.get('heart-filled'))).toMatch(/filled[/]heart[.]svg$/)
    for (const name of icons.keys()) expect(name).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
  })
})
