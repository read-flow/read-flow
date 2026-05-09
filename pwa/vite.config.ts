import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit(),
		VitePWA({
			registerType: 'autoUpdate',
			// Service worker is only active in the production build.
			// Use `npm run preview` to test PWA features locally.
			manifest: {
				name: 'Read Flow',
				short_name: 'Read Flow',
				description: 'Personal document library and reader',
				theme_color: '#1e293b',
				background_color: '#f8fafc',
				display: 'standalone',
				start_url: '/',
				icons: [
					{
						src: '/icons/pwa-192x192.png',
						sizes: '192x192',
						type: 'image/png',
					},
					{
						src: '/icons/pwa-512x512.png',
						sizes: '512x512',
						type: 'image/png',
					},
					{
						src: '/icons/pwa-512x512.png',
						sizes: '512x512',
						type: 'image/png',
						purpose: 'any maskable',
					},
				],
			},
			workbox: {
				// Precache the built app shell only. API calls (to remote read-flow
				// servers) are not cached — they must always come from live sources.
				globPatterns: ['**/*.{js,css,html,ico,png,svg,woff,woff2}'],
			},
		}),
	],
});
