// The icon library the page draws from, gathered at build time.
//
// Every icon is known by one name in one flat name space. Where two sources have the same name
// the first one listed wins: Overseer's own icons (this folder, under the names buttons have
// always used), then Lucide, then Tabler's outline icons, then Tabler's filled ones, which take
// a `-filled` suffix. The build copies the merged set into `dist/icons/<name>.svg`, so the page,
// the server and the desktop app all reach an icon by the same address.
//
// This runs in Node, from vite.config.js, never in the page.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const modules = path.resolve(here, '../../node_modules')

export const ICON_SOURCES = [
    { dir: here, suffix: '' },
    { dir: path.join(modules, 'lucide-static/icons'), suffix: '' },
    { dir: path.join(modules, '@tabler/icons/icons/outline'), suffix: '' },
    { dir: path.join(modules, '@tabler/icons/icons/filled'), suffix: '-filled' },
]

// The licences that travel with the icons, as the sets' own terms ask.
export const ICON_LICENSES = [
    { from: path.join(modules, 'lucide-static/LICENSE'), to: 'lucide.txt' },
    { from: path.join(modules, '@tabler/icons/LICENSE'), to: 'tabler.txt' },
]

// Only plain names: nothing that could climb out of the icon folder.
export const ICON_NAME = /^[a-z0-9]+(-[a-z0-9]+)*$/

/** The file an icon name stands for, or null when no source has it. */
export function resolveIcon(name) {
    if (typeof name !== 'string' || !ICON_NAME.test(name)) return null
    for (const { dir, suffix } of ICON_SOURCES) {
        if (suffix && !name.endsWith(suffix)) continue
        const file = path.join(dir, `${name.slice(0, name.length - suffix.length)}.svg`)
        if (fs.existsSync(file)) return file
    }
    return null
}

/** Every icon name, each with the file the first source to have it holds. */
export function collectIcons() {
    const icons = new Map()
    for (const { dir, suffix } of ICON_SOURCES) {
        if (!fs.existsSync(dir)) continue
        for (const entry of fs.readdirSync(dir)) {
            if (!entry.endsWith('.svg')) continue
            const name = entry.slice(0, -4) + suffix
            if (ICON_NAME.test(name) && !icons.has(name)) icons.set(name, path.join(dir, entry))
        }
    }
    return icons
}

/** Every icon name, in order, as the page asks for them to offer a choice of icon. */
export function iconIndex(icons = collectIcons()) {
    return JSON.stringify([...icons.keys()].sort())
}

/**
 * Copy the merged library, and the licences it comes under, into `<outDir>/icons`, with
 * `index.json`, the list of its names.
 */
export function writeIconLibrary(outDir) {
    const target = path.join(outDir, 'icons')
    fs.mkdirSync(path.join(target, 'LICENSES'), { recursive: true })
    const icons = collectIcons()
    for (const [name, file] of icons) fs.copyFileSync(file, path.join(target, `${name}.svg`))
    fs.writeFileSync(path.join(target, 'index.json'), iconIndex(icons))
    for (const { from, to } of ICON_LICENSES) {
        if (fs.existsSync(from)) fs.copyFileSync(from, path.join(target, 'LICENSES', to))
    }
    return icons.size
}
