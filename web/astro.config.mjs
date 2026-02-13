// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://helmapp.dev',
	integrations: [
		starlight({
			title: 'Helm',
			tagline: 'Take the helm.',
			logo: {
				src: './src/assets/helm-icon.png',
			},
			social: [
				{ label: 'GitHub', href: 'https://github.com/jasoncavinder/Helm', icon: 'github' },
			],
			sidebar: [
				{ label: 'Overview', slug: 'overview' },
				{
					label: 'Guides',
					items: [
						{ label: 'Installation', slug: 'guides/installation' },
						{ label: 'Usage', slug: 'guides/usage' },
					],
				},
				{ label: 'Roadmap', slug: 'roadmap' },
			],
		}),
	],
});
