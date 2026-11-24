import { readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { SKILL_DIR, BUNDLED } from '../scripts/sync-skill.mjs'

const REPO = resolve(__dirname, '..')

// A checkout may give either file CRLF line endings; the copy is current if only those differ.
const text = (path) => readFileSync(path, 'utf-8').replace(/\r\n/g, '\n')

describe('the overseer-documents skill', () => {
    it.each(BUNDLED)('carries the current %s (run node scripts/sync-skill.mjs)', (file) => {
        const source = text(join(REPO, file))
        const copy = text(join(SKILL_DIR, file))
        expect(copy === source).toBe(true)
    })
})
