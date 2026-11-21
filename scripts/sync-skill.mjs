#!/usr/bin/env node
// Copies the specification and API into the overseer-documents skill, so the skill folder works
// on its own once copied out of the repository. The root files stay the ones to edit.
import { copyFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const SKILL_DIR = join(REPO, '.claude', 'skills', 'overseer-documents')
export const BUNDLED = ['overseer_syntax_specification.md', 'API.md']

if (process.argv[1] === fileURLToPath(import.meta.url)) {
    for (const file of BUNDLED) copyFileSync(join(REPO, file), join(SKILL_DIR, file))
    console.log(`copied ${BUNDLED.join(', ')} into ${SKILL_DIR}`)
}
