// Steps for preview.mjs: look at a project tracker, file a task through its form, move an item
// along, and read back what reached the file.
//
//   node scripts/preview.mjs examples/projects/project_template.os --script scripts/preview_example.mjs

await shot('whole')

log('parser is at', await text('project/Items/[parser]/stage'))
await pick('project/Items/[parser]/stage', 'planning')
log('and now at', await text('project/Items/[parser]/stage'))

await press('project/NewTask/title')
await type('Draw the stage chips larger')
await press('project/NewTask/add')
log('the first item is now', await text('project/Items/[0]/title'))
log('its stage field, as the page holds it:', await node('project/Items/[0]/stage', 0))

await shot('items', { of: 'project/Items' })
log('what the file gained and lost:', await changes())
