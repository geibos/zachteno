import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

const backend = process.env.BACKEND_URL ?? 'http://localhost:8080';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	server: {
		port: 5173,
		strictPort: true,
		proxy: {
			// Origin не переписываем: бэкенд в dev разрешает http://localhost:5173.
			'/api': { target: backend },
			'/health': { target: backend }
		}
	},
	test: {
		include: ['src/**/*.{test,spec}.{js,ts}'],
		environment: 'node'
	}
});
