// Node script to run Vitest (frontend) and Cargo tests (backend) and print a combined summary
const { spawn } = require('child_process')

function run(cmd, args, options = {}) {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, { stdio: ['ignore', 'pipe', 'pipe'], shell: false, ...options })
    let out = ''
    let err = ''
    child.stdout.on('data', (d) => { process.stdout.write(d); out += d.toString() })
    child.stderr.on('data', (d) => { process.stderr.write(d); err += d.toString() })
    child.on('close', (code) => resolve({ code, out, err }))
  })
}

;(async () => {
  // Run Vitest
  const vitest = await run(process.platform === 'win32' ? 'cmd.exe' : 'sh', process.platform === 'win32' ? ['/d', '/s', '/c', 'npx vitest run -c tests/vitest.config.mjs'] : ['-lc', 'npx vitest run -c tests/vitest.config.mjs'])
  // Parse Vitest summary (robust to different formats/whitespace/colors)
  let jsFiles = 0, jsTests = 0, jsFailed = 0
  try {
    const stripAnsi = (s) => s.replace(/\x1b\[[0-9;]*m/g, '')
    const cleanOut = stripAnsi(vitest.out || '')
    const lines = cleanOut.split(/\r?\n/)
    const filesLine = lines.find(l => /Test Files\b/.test(l)) || ''
    const testsLine = lines.find(l => /^\s*Tests\b/.test(l)) || ''
    // Prefer the total inside parentheses when present, else first number on the line
    const parenNum = (s) => { const m = s.match(/\((\d+)\)/); return m ? parseInt(m[1], 10) : null }
    const firstNum = (s) => { const m = s.match(/(\d+)/); return m ? parseInt(m[1], 10) : null }
    const filesParen = parenNum(filesLine)
    const testsParen = parenNum(testsLine)
    const filesAny = filesParen ?? firstNum(filesLine) ?? 0
    const testsAny = testsParen ?? firstNum(testsLine) ?? 0
    jsFiles = filesAny || 0
    jsTests = testsAny || 0
    // Failed tests: look for an explicit "Failed Tests" line or for the word 'failed' on the Tests line
    const failedLine = lines.find(l => /Failed Tests\b/i.test(l)) || ''
    let failed = 0
    if (failedLine) {
      const mf = failedLine.match(/Failed Tests\s+(\d+)/i)
      if (mf) failed = parseInt(mf[1], 10) || 0
    } else if (/failed/i.test(testsLine)) {
      const mf2 = testsLine.match(/(\d+)\s+failed/i)
      if (mf2) failed = parseInt(mf2[1], 10) || 0
    }
    jsFailed = failed
  } catch {}

  // Run cargo tests
  const cargo = await run(process.platform === 'win32' ? 'cmd.exe' : 'sh', process.platform === 'win32' ? ['/d', '/s', '/c', 'cd src-tauri && cargo test'] : ['-lc', 'cd src-tauri && cargo test'])
  // Parse cargo summary line
  let rsPassed = 0, rsFailed = 0
  try {
    const lines = cargo.out.split(/\r?\n/)
    const last = lines.reverse().find(l => /test result:/.test(l)) || ''
    let mp = last.match(/test result: ok\. (\d+) passed; (\d+) failed/)
    if (!mp) mp = last.match(/test result: ok\. (\d+) passed;/)
    if (mp) {
      rsPassed = parseInt(mp[1], 10) || 0
      rsFailed = mp[2] ? (parseInt(mp[2], 10) || 0) : 0
    }
  } catch {}

  const totalPassed = jsTests + rsPassed
  const totalFailed = jsFailed + rsFailed
  console.log(`\nAll tests summary: ${totalPassed} passed; ${totalFailed} failed; suites(js=${jsFiles})`)
  process.exit((vitest.code || 0) !== 0 || (cargo.code || 0) !== 0 ? 1 : 0)
})().catch((e) => { console.error(e); process.exit(1) })
