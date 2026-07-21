import { defineConfig } from 'vitest/config';

// Pure-logic unit tests only (e.g. src/lib/slides.ts). Component/DOM tests are
// out of scope for now; browser flows are covered by the acceptance gate.
export default defineConfig({
	test: {
		environment: 'node',
		include: ['src/**/*.test.ts']
	}
});
