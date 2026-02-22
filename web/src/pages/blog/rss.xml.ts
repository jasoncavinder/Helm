import type { APIRoute } from 'astro';
import { getCollection } from 'astro:content';

const SITE_URL = 'https://helmapp.dev';
const BLOG_TITLE = 'Helm Blog';
const BLOG_DESCRIPTION = 'Product updates, release highlights, and operator guidance from the Helm team.';

function escapeXml(value: string): string {
	return value
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;')
		.replaceAll("'", '&apos;');
}

function docsPathFromId(id: string): string {
	const withoutExtension = id.replace(/\.(md|mdx)$/i, '');
	const withoutIndex = withoutExtension.replace(/\/index$/i, '');
	return `/${withoutIndex}/`;
}

function inferDateFromId(id: string): Date | null {
	const match = id.match(/(\d{4})-(\d{2})-(\d{2})/);
	if (!match) return null;
	const [, year, month, day] = match;
	const parsed = new Date(`${year}-${month}-${day}T00:00:00Z`);
	return Number.isNaN(parsed.getTime()) ? null : parsed;
}

export const GET: APIRoute = async () => {
	const entries = await getCollection('docs', ({ id, data }) => {
		if (!id.startsWith('blog/')) return false;
		if (id.endsWith('/index.md') || id.endsWith('/index.mdx')) return false;
		return data.draft !== true;
	});

	const posts = entries
		.map((entry) => ({
			title: entry.data.title,
			description: entry.data.summary ?? entry.data.description ?? '',
			date: entry.data.publishDate ?? inferDateFromId(entry.id) ?? new Date(),
			link: new URL(docsPathFromId(entry.id), SITE_URL).toString(),
			guid: entry.id,
		}))
		.sort((left, right) => right.date.getTime() - left.date.getTime());

	const items = posts
		.map(
			(post) => `<item>
	<title>${escapeXml(post.title)}</title>
	<link>${escapeXml(post.link)}</link>
	<guid>${escapeXml(post.guid)}</guid>
	<pubDate>${post.date.toUTCString()}</pubDate>
	<description>${escapeXml(post.description)}</description>
</item>`
		)
		.join('\n');

	const lastBuildDate = posts[0]?.date ?? new Date();
	const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
	<title>${escapeXml(BLOG_TITLE)}</title>
	<link>${SITE_URL}/blog/</link>
	<description>${escapeXml(BLOG_DESCRIPTION)}</description>
	<language>en-us</language>
	<lastBuildDate>${lastBuildDate.toUTCString()}</lastBuildDate>
	${items}
</channel>
</rss>`;

	return new Response(xml, {
		headers: {
			'Content-Type': 'application/rss+xml; charset=utf-8',
			'Cache-Control': 'public, max-age=600',
		},
	});
};
