const BLOG_DIRECTORY_DEFAULT = 'blog';
const BLOG_LABEL_DEFAULT = 'Blog';
const BLOG_RSS_PATH_DEFAULT = '/blog/rss.xml';

function isObject(value) {
	return typeof value === 'object' && value !== null;
}

function hasBlogSidebarEntry(sidebar, directory, label) {
	return sidebar.some((entry) => {
		if (!isObject(entry)) return false;
		if (isObject(entry.autogenerate) && entry.autogenerate.directory === directory) return true;
		if (typeof entry.label === 'string' && entry.label.toLowerCase() === label.toLowerCase()) return true;
		return false;
	});
}

function findChangelogIndex(sidebar) {
	return sidebar.findIndex((entry) => {
		if (!isObject(entry)) return false;
		if (entry.slug === 'changelog') return true;
		if (entry.link === '/changelog/' || entry.link === '/changelog') return true;
		if (typeof entry.label === 'string' && entry.label.toLowerCase() === 'changelog') return true;
		return false;
	});
}

function hasSocialLink(social, href) {
	return social.some((entry) => isObject(entry) && typeof entry.href === 'string' && entry.href === href);
}

export function helmStarlightBlogPlugin(options = {}) {
	const blogDirectory = options.blogDirectory ?? BLOG_DIRECTORY_DEFAULT;
	const blogLabel = options.blogLabel ?? BLOG_LABEL_DEFAULT;
	const blogRssPath = options.blogRssPath ?? BLOG_RSS_PATH_DEFAULT;

	return {
		name: 'helm-starlight-blog-plugin',
		hooks: {
			'config:setup'({ config, updateConfig }) {
				const nextConfig = {};

				const sidebar = Array.isArray(config.sidebar) ? [...config.sidebar] : [];
				if (!hasBlogSidebarEntry(sidebar, blogDirectory, blogLabel)) {
					const blogEntry = {
						label: blogLabel,
						autogenerate: {
							directory: blogDirectory,
							collapsed: false,
							attrs: {},
						},
					};
					const changelogIndex = findChangelogIndex(sidebar);
					if (changelogIndex === -1) {
						sidebar.push(blogEntry);
					} else {
						sidebar.splice(changelogIndex, 0, blogEntry);
					}
					nextConfig.sidebar = sidebar;
				}

				const social = Array.isArray(config.social) ? [...config.social] : [];
				if (!hasSocialLink(social, blogRssPath)) {
					social.push({
						label: 'RSS',
						href: blogRssPath,
						icon: 'rss',
					});
					nextConfig.social = social;
				}

				if (Object.keys(nextConfig).length > 0) {
					updateConfig(nextConfig);
				}
			},
		},
	};
}
