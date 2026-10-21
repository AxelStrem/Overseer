// The version of a document's text, worked out the way the backend works it out - see
// `app_api::version_of`: 64-bit FNV-1a over the text's UTF-8 bytes, then its length in bytes.
//
// The page names the text it is showing with every change it sends, so the answer is described
// against what it holds - and it may hold the text itself rather than a version handed to it.
// Two halves of 32 bits rather than a BigInt, which costs seconds on a document of megabytes.

const encoder = new TextEncoder()

export function versionOf(text) {
    const bytes = encoder.encode(text)
    let high = 0xcbf29ce4
    let low = 0x84222325
    for (let i = 0; i < bytes.length; i += 1) {
        low = (low ^ bytes[i]) >>> 0
        // Times 0x100000001b3, which is 0x1b3 plus 2^40: each product below stays under 2^42,
        // where a double is still exact.
        const lowProduct = low * 0x1b3
        const carry = Math.floor(lowProduct / 0x100000000)
        high = (high * 0x1b3 + carry + ((low << 8) >>> 0)) >>> 0
        low = lowProduct >>> 0
    }
    const hex = (n) => n.toString(16).padStart(8, '0')
    return `${hex(high)}${hex(low)}-${bytes.length}`
}
