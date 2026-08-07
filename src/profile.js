// Opt-in timing for a single interaction, shared by the app and the renderer.
//
// Enable from the devtools console with
//   localStorage.overseerProfile = '1'
// then reload; disable with localStorage.removeItem('overseerProfile').
//
// Note that having devtools open distorts what is measured here: it instruments the IPC, so a
// backend round trip reads several times its real cost. Read these numbers with the console
// closed and the log inspected afterwards, or treat the relative sizes rather than the
// absolute ones as the signal.

export const PROFILE = (() => {
    try {
        return typeof localStorage !== 'undefined' && localStorage.getItem('overseerProfile') === '1'
    } catch { return false }
})()

export const now = () => (typeof performance !== 'undefined' ? performance.now() : Date.now())

/** Report the time since `startedAt` and return the current time, for chaining. */
export function profileMark(label, startedAt) {
    const t = now()
    // console.warn rather than log, which the app silences outside debug mode.
    if (PROFILE) console.warn(`[profile] ${label} ${(t - startedAt).toFixed(1)} ms`)
    return t
}

/** Time a synchronous call and report it under `label`. */
export function profiled(label, fn) {
    if (!PROFILE) return fn()
    const started = now()
    try { return fn() } finally { profileMark(label, started) }
}
