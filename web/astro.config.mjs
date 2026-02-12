// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://jasoncavinder.github.io',
	base: '/Helm',
	integrations: [
		starlight({
			title: 'Helm',
			social: {
				github: 'https://github.com/jasoncavinder/Helm',
			},
			sidebar: [
				{
					label: 'Start Here',
					items: [
						{ label: 'Introduction', slug: 'guides/example' },
					],
				},
			],
		}),
	],
});
