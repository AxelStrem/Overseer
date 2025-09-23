import { describe, it, expect, beforeEach, vi } from 'vitest'

// Minimal DOM container expected by renderer
function setupDOM() {
  document.body.innerHTML = `
    <div id="app">
      <div id="toolbar">
        <button id="open-file-btn"></button>
        <button id="new-file-btn"></button>
        <button id="save-file-btn"></button>
        <button id="reload-file-btn"></button>
      </div>
      <button id="welcome-open-btn"></button>
      <button id="welcome-new-btn"></button>
      <button id="error-back-btn"></button>
      <div id="tab-container"></div>
      <div id="content-display"></div>
      <div id="status-bar">
        <span id="status-message"></span>
        <span id="status-info"></span>
        <span id="file-path"></span>
      </div>
      <div id="welcome-screen" class="screen"></div>
      <div id="editor-screen" class="screen"></div>
      <div id="error-screen" class="screen"></div>
      <div id="error-message"></div>
    </div>
  `
}

vi.mock('@tauri-apps/api/tauri', () => ({ invoke: vi.fn() }))

import { OverseerApp } from '../src/main.js'

// Utility to deeply clone plain objects
const deepClone = (o) => JSON.parse(JSON.stringify(o))

// Load the raw source text of the weight tracker example. In real app this would come from FS.
// We inline the content here to ensure the test is self-contained and asserts exact round-trip fidelity.
const ORIGINAL_SOURCE = `tab weight_minimal {\n\n    // Minimal focus: a selected date (day precision) and Prev/Next navigation\n    div (hidden=true) {\n\n        div MealRecord (layout=\"vertical\") {\n            div {\n                string description = \"\"\n                int amount (label=\"Amount\") = 1\n                text Score (font-size=100px, margin=0px) = $((NutriScore/S <= 0.0)?\n                    \"# <color= #329c17 | A>\":((NutriScore/S <= 2.0)?\n                    \"# <color= #78cd11 | B>\":((NutriScore/S <= 10.0)?\n                    \"# <color= #cdca26 | C>\":((NutriScore/S <= 18.0)?\n                    \"# <color= #f58412 | D>\":\n                    \"# <color= #cc2512 | E>\"))))\n            }\n            div {\n                float calories (label=\"Calories\") = $(amount*per_item/calories)\n                float weight (label=\"Weight\", suffix=\" g\") = $(amount*per_item/weight)\n                float protein (label=\"Protein\") = $(amount*per_item/protein)\n                float fat (label=\"Fat\") = $(amount*per_item/fat)\n                float saturated_fat (label=\"Saturated Fat\") = $(amount*per_item/saturated_fat)\n                float carbs (label=\"Carbs\") = $(amount*per_item/carbs)\n                float sugar (label=\"Sugar\") = $(amount*per_item/sugar)\n                float fibre (label=\"Fibre\") = $(amount*per_item/fibre)\n                float salt (label=\"Salt\") = $(amount*per_item/salt)\n\n                div NutriScore (hidden=true) {\n                    float A = $(per_100g/calories*0.0125)\n                    float B = $(per_100g/sugar*0.22222)\n                    float C = $(per_100g/saturated_fat)\n                    float D = $(per_100g/salt*11.11)\n                    float F = $(per_100g/fibre*1.43)\n                    float G = $(per_100g/protein*0.625)\n\n                    float S = $(A + B + C + D - F - G)\n                }\n            }\n\n            div per_item {\n                float calories = $(weight*per_100g/calories*0.01)\n                float weight (suffix=\" g\") = 100\n                float protein = $(weight*per_100g/protein*0.01)\n                float fat = $(weight*per_100g/fat*0.01)\n                float saturated_fat = $(weight*per_100g/saturated_fat*0.01)\n                float carbs = $(weight*per_100g/carbs*0.01)\n                float sugar = $(weight*per_100g/sugar*0.01)\n                float fibre = $(weight*per_100g/fibre*0.01)\n                float salt = $(weight*per_100g/salt*0.01)\n            }\n\n            div per_100g {\n                float calories = 100\n                float protein = 5\n                float fat = 5\n                float saturated_fat = 1\n                float carbs = 5\n                float sugar = 1\n                float fibre = 10\n                float salt = 1\n            }\n        }\n\n        div WeightRecord (background-color=$((total_calories < 1000) ?\"#195700ff\":\"#3f0803ff\")) { // Daily data entry\n            timestamp date (precision=\"day\") = $(today())\n            string test_data = \"test\"\n            float weight (fallback=$(\n                /weight_minimal/History\n                    .filter(|x| x/date == /weight_minimal/History.filter(|x| x/date < ../date).map(|x| x/date).max())\n                    .map(|x| x/weight)\n                    .first(80.0)\n            ), precision=1, suffix=\" kg\") = null\n\n            int total_calories (label=\"Total Calories\") = $(intake.sum(calories))\n            int total_protein (label=\"Total Protein\") = $(intake.sum(protein))\n\n            list intake (entry=<MealRecord>, hidden=true, layout=\"vertical\")\n        }\n    }\n\n    // Selected panel with only the selected date\n    div Selected (layout=\"vertical\") {\n        //div \n            // Dynamic linking handles load and create-on-edit; no manual population needed\n        button Prev (label=\"< Prev Day\") {\n            on click {\n                set (path=\"/weight_minimal/Selected/selected_date\") = $(date_add_days(../selected_date, -1))\n            }\n        }\n        timestamp selected_date (precision=\"day\") = $(today())\n\n        button Next (label=\"> Next Day\") {\n            on click {\n                set (path=\"/weight_minimal/Selected/selected_date\") = $(date_add_days(../selected_date, 1))\n            }\n        }\n        // \n            \n        // Override the linked WeightRecord's intake visibility locally\n        // Prepend the new record on first edit if it's missing\n        div SelectedWeightRecord (link=\"/weight_minimal/History[key=$(../selected_date)]\", phantom-materialize=\"prepend-on-edit\", background-color=\"#000000\") {\n            list intake (hidden=false)\n        }\n    }\n\n    // History uses date as key and day precision for equivalence\n    list History (entry=<WeightRecord>, key=\"date\", keyPrecision=\"day\") {\n        - {\n            - date = \"2025-09-23\"\n            - weight = 109.3\n        }\n        - {\n            - date = \"2025-09-22\"\n            - test_data = \"test\"\n            - weight = 108.8\n            - total_calories = $(intake.sum(calories))\n            - intake {\n                - {\n                    - description = \"Chicken Wrap\"\n                    - amount = 1\n                    div per_item {\n                        - weight = 300\n                        - calories = 410\n                        - fat = 23\n                        - saturated_fat = 3\n                        - salt = 0.340\n                        - carbs = 19\n                        - sugar = 4\n                        - fibre = 5\n                        - protein = 27\n                    }\n                }\n                - {\n                    - description = \"Coffee\"\n                    - amount = 1\n                    div per_item {\n                        - weight = 250\n                    }\n                    div per_100g {\n                        - calories = 80\n                        - protein = 0\n                        - fat = 10\n                        - saturated_fat = 2\n                        - carbs = 20\n                        - sugar = 5\n                        - fibre = 0\n                        - salt = 0\n                    }\n                }\n            }\n        }\n        - {\n            - date = \"2025-09-21\"\n            - test_data = \"test\"\n            - weight = 109\n            - total_calories = $(intake.sum(calories))\n            list intake {\n                - {\n                    - description = \"Pizza Slice\"\n                    - amount = 5\n                    div per_100g {\n                        float calories = 340\n                        float protein = 3\n                        float fat = 12\n                        float saturated_fat = 4\n                        float carbs = 62\n                        float sugar = 3\n                        float fibre = 3\n                        float salt = 1\n                    }\n                }\n                - {\n                    - description = \"Potato Salad\"\n                    - amount = 1\n                    div per_item {\n                        float calories = $(weight*per_100g/calories*0.01)\n                        float weight = 200\n                        float protein = $(weight*per_100g/protein*0.01)\n                        float fat = $(weight*per_100g/fat*0.01)\n                        float saturated_fat = $(weight*per_100g/saturated_fat*0.01)\n                        float carbs = $(weight*per_100g/carbs*0.01)\n                        float sugar = $(weight*per_100g/sugar*0.01)\n                        float fibre = $(weight*per_100g/fibre*0.01)\n                        float salt = $(weight*per_100g/salt*0.01)\n                    }\n                    div per_100g {\n                        float calories = 150\n                        float protein = 5\n                        float fat = 5\n                        float saturated_fat = 1\n                        float carbs = 5\n                        float sugar = 1\n                        float fibre = 10\n                        float salt = 1\n                    }\n                }\n            }\n        }\n        - {\n            - date = \"2025-09-20\"\n        }\n        - {\n            - date = \"2025-09-19\"\n            - test_data = \"test\"\n            - weight = 108.4\n            - total_calories = $(intake.sum(calories))\n        }\n        - {\n            - date = \"2025-09-18\"\n            - test_data = \"test\"\n            - weight = 108.5\n            - total_calories = $(intake.sum(calories))\n        }\n        - {\n            - date = \"2025-09-17\"\n            - test_data = \"test\"\n            - weight = 108.7\n            - total_calories = $(intake.sum(calories))\n        }\n        - {\n            - date = \"2025-09-16\"\n            - test_data = \"test\"\n            - weight = 108.4\n            - total_calories = $(intake.sum(calories))\n        }\n        - {\n            - date = \"2025-09-13\"\n            - test_data = \"test\"\n            - weight = 107.8\n            - total_calories = $(intake.sum(calories))\n        }\n        - {\n            - date = \"2025-09-12\"\n            - test_data = \"test\"\n            - weight = 109.2\n            - total_calories = $(intake.sum(calories))\n        }\n        - {\n            - date = \"2025-09-11\"\n            - weight = 109.3\n            - total_calories = 2000\n        }\n        - {\n            - date = \"2025-09-09\"\n            - test_data = \"Tuesday\"\n            - weight = 108.8\n            list intake {\n                - {\n                    - description = \"Apple\"\n                    div per_item {\n                        float calories = 60\n                        float weight = 200\n                        float protein = 5\n                        float fat = 5\n                        float saturated_fat = $(weight*per_100g/saturated_fat*0.01)\n                        float carbs = 5\n                        float sugar = $(weight*per_100g/sugar*0.01)\n                        float fibre = $(weight*per_100g/fibre*0.01)\n                        float salt = $(weight*per_100g/salt*0.01)\n                    }\n                }\n            }\n        }\n        - {\n            - date = \"2025-09-07\"\n            - test_data = \"W\"\n            - weight = 79.8\n        }\n        - {\n            - date = \"2025.08.27\"\n            - test_data = \"Wednesday\"\n            - weight = 79.5\n            list intake {\n                - {\n                    - calories = 100\n                }\n                - {\n                    - calories = 150\n                }\n            }\n        }\n        - {\n            - date = \"2025.08.26\"\n            - test_data = \"Wednesday\"\n            - weight = 80.1\n        }\n        - {\n            - date = \"2025.08.25\"\n            - test_data = \"Tuesday\"\n            - weight = 80\n        }\n        - {\n            - date = \"2025.08.24\"\n            - test_data = \"Monday\"\n            - weight = 108.8\n        }\n    }\n}\n\n`

// Helper to normalize line endings for comparison (avoid Windows vs Unix issues)
function normalizeNewlines(str) { return str.replace(/\r\n/g, '\n') }

// The serializer may canonicalize some trivial whitespace (e.g., trailing spaces). For now we assert exact match; if needed we can relax by trimming trailing spaces per line.
function canonicalize(str) {
  return normalizeNewlines(str)
    .split('\n')
    .map(l => l.replace(/\s+$/,'') ) // strip trailing spaces only
    .join('\n')
}

describe('Round-trip template fidelity (no unintended materialization)', () => {
  beforeEach(() => setupDOM())

  it('re-serializing weight_tracker_new without edits yields identical text', async () => {
    const { invoke } = await import('@tauri-apps/api/tauri')

    // We'll capture the last serialized text
    let lastSerializedText = null
    // Simulated parsed document structure - for this fidelity test we only need the serialized text path.
    // The backend in real run would produce a structured JSON; here we stub parse to return an empty array
    // because renderer operations for no-edit load shouldn't depend on internal nodes for the save path assertion.
    let currentDoc = []

    invoke.mockImplementation((cmd, args) => {
      if (cmd === 'load_overseer_file') return Promise.resolve(ORIGINAL_SOURCE)
      if (cmd === 'parse_overseer_content') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'parse_overseer_content_selective') return Promise.resolve(deepClone(currentDoc))
      if (cmd === 'get_next_timer_due_ms') return Promise.resolve(null)
      if (cmd === 'scheduler_tick') return Promise.resolve(null)
      if (cmd === 'serialize_overseer_nodes') { return Promise.resolve('REGENERATED_PLACEHOLDER') }
      if (cmd === 'save_overseer_file_with_original') {
        // Backend merges comments internally; for fidelity we approximate by capturing regenerated param.
        // If serializer introduces unintended materialization, regenerated will differ from ORIGINAL_SOURCE canon.
        lastSerializedText = args.regenerated
        return Promise.resolve(null)
      }
      return Promise.reject(new Error('unknown command: '+cmd))
    })

    const app = new OverseerApp()
    // Simulate open
    app.currentFilePath = 'weight_tracker_new.os'
    await app.loadFile('weight_tracker_new.os')

    // Immediately save without edits
    await app.saveFile()

    expect(lastSerializedText).toBeTruthy()

    const originalCanon = canonicalize(ORIGINAL_SOURCE)
    const savedCanon = canonicalize(lastSerializedText)

    // Core assertion: they must be identical (test will fail until serializer suppression logic is correct)
    expect(savedCanon).toBe(originalCanon)
  })
})
