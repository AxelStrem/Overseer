// A fake backend for specs written when the page sent the document itself to be worked out.
//
// The page sends the document's text now, never the document, and the answers to the text
// commands carry the text back. These specs fake a backend that knew the document from what
// the page sent; this answers the text commands from the same fake, given the document as the
// page holds it - which is what the page used to send.
//
// The spec gives the page a text to start from (`app._currentText`), as opening a document does.
export function answeringFromText(older, text = 'DOC') {
    const clone = (o) => JSON.parse(JSON.stringify(o))
    const sent = () => {
        const doc = window.app && window.app.currentDocument
        return clone(Array.isArray(doc) ? doc : [doc])
    }
    // What the older fake made of the document being sent to be serialized, if anything.
    const told = async () => {
        try { await older('serialize_overseer_nodes', { nodes: sent() }) } catch (_) { /* not faked */ }
    }
    return async (cmd, args) => {
        if (cmd === 'parse_overseer_content_selective_with_text') {
            await told()
            const nodes = await older('parse_overseer_content_selective', args)
            return { nodes, text }
        }
        if (cmd === 'execute_overseer_event_with_text') {
            await told()
            const nodes = await older('execute_overseer_event', { ...args, nodes: sent() })
            return Array.isArray(nodes) ? { nodes, text } : null
        }
        return older(cmd, args)
    }
}
