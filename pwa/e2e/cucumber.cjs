/** @cucumber/cucumber config — see https://github.com/cucumber/cucumber-js/blob/main/docs/configuration.md */
module.exports = {
	default: {
		paths: ['../features/**/*.feature'],
		import: ['e2e/support/**/*.ts', 'e2e/steps/**/*.ts'],
		// Run scenarios with no driver tag (PWA canary) plus any explicitly
		// tagged @pwa (shared cross-surface scenarios) — skip @rest/@cosmic-only
		// canaries that have no PWA analogue (e.g. `_smoke_rest.feature`).
		tags: 'not @rest and not @cosmic or @pwa',
		format: ['progress-bar'],
		publishQuiet: true,
	},
};
