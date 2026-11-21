import { readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { SKILL_DIR, BUNDLED } from '../scripts/sync-skill.mjs'

const REPO = resolve(__dirname, '..')

describe('the overseer-documents skill', () => {
    it.each(BUNDLED)('carries the current %s (run node scripts/sync-skill.mjs)', (file) => {
        const source = readFileSync(join(REPO, file), 'utf-8')
        const copy = readFileSync(join(SKILL_DIR, file), 'utf-8')
        expect(copy === source).toBe(true)
    })
})
