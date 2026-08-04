import { JSDOM } from 'jsdom'
import { installTauriBridge } from './tauri-bridge.js'

export function bootstrapMinimalDom() {
  const html = `<!doctype html>
  <html>
    <head></head>
    <body>
      <div id="status-bar"><span id="status-message"></span><span id="status-info"></span></div>
      <div id="welcome-screen" class="screen active"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
      <div id="file-path"></div>
      <div id="tab-container"></div>
      <div id="content-display"></div>
      <button id="open-file-btn"></button>
      <button id="new-file-btn"></button>
      <button id="save-file-btn"></button>
      <button id="reload-file-btn"></button>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
    </body>
  </html>`
  const { window } = new JSDOM(html, { url: 'http://localhost/' })
  // This replaces the window the global setup ran against, so the IPC bridge
  // has to come along with it.
  installTauriBridge(window)
  global.window = window
  global.document = window.document
  return window
}
