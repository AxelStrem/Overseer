JavaScript tests

- Uses Vitest with jsdom environment.
- Run all tests (JS + Rust): npm test
- Run JS tests only: npm run test:js

The list_corruption.spec.js test guards against regression where selective updates would re-render a list subtree using generic '-' items, losing template instance styling.
