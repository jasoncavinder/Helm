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
			customCss: ['./src/styles/helm-theme.css'],
			logo: {
				src: './src/assets/helm-icon.png',
			},
			social: [
				{ label: 'GitHub', href: 'https://github.com/jasoncavinder/Helm', icon: 'github' },
			],
			lastUpdated: true,
			head: [
				{
					tag: 'meta',
					attrs: {
						property: 'og:image',
						content: 'https://helmapp.dev/og-image.png',
					},
				},
				{
					tag: 'meta',
					attrs: {
						name: 'twitter:card',
						content: 'summary_large_image',
					},
				},
				{
					tag: 'meta',
					attrs: {
						name: 'twitter:image',
						content: 'https://helmapp.dev/og-image.png',
					},
				},
			],
			sidebar: [
				{ label: 'Overview', link: '/product-overview/' },
				{
					label: 'Guides',
					items: [
						{ label: 'Installation', slug: 'guides/installation' },
						{ label: 'Usage', slug: 'guides/usage' },
						{ label: 'Visual Tour', slug: 'guides/visual-tour' },
						{ label: 'FAQ & Troubleshooting', slug: 'guides/faq' },
					],
				},
				{ label: 'Changelog', slug: 'changelog' },
				{ label: 'Roadmap', link: '/product-roadmap/' },
				{ label: 'Licensing', slug: 'licensing' },
			],
		}),
	],
});
