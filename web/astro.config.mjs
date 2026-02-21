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
			favicon: '/favicon.ico',
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
					tag: 'link',
					attrs: {
						rel: 'apple-touch-icon',
						href: '/apple-touch-icon.png',
					},
				},
				{
					tag: 'meta',
					attrs: {
						property: 'og:image',
						content: 'https://helmapp.dev/og-image.png?v=2',
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
						content: 'https://helmapp.dev/og-image.png?v=2',
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
