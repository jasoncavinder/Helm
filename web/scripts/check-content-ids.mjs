import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const guidesDir = path.resolve(scriptDir, '..', 'src', 'content', 'docs', 'guides');
const watchedGuideIds = ['guides/faq', 'guides/installation', 'guides/usage'];

function normalizeSlug(value) {
	return value.replace(/^\/+/, '').replace(/\/+$/, '');
}

function parseFrontmatterSlug(filePath) {
	const raw = fs.readFileSync(filePath, 'utf8');
	const frontmatterMatch = raw.match(/^---\s*\n([\s\S]*?)\n---\s*(?:\n|$)/);
	if (!frontmatterMatch) {
		return null;
	}
	const slugMatch = frontmatterMatch[1].match(/^\s*slug:\s*["']?([^"'\n]+)["']?\s*$/m);
	if (!slugMatch) {
		return null;
	}
	return normalizeSlug(slugMatch[1].trim());
}

function inferDocId(filePath) {
	const basename = path.basename(filePath, path.extname(filePath));
	const explicitSlug = parseFrontmatterSlug(filePath);
	if (explicitSlug) {
		return explicitSlug;
	}
	return `guides/${basename}`;
}

const files = fs
	.readdirSync(guidesDir)
	.filter((name) => name.endsWith('.md') || name.endsWith('.mdx'))
	.map((name) => path.join(guidesDir, name));

const idMap = new Map();
for (const filePath of files) {
	const id = inferDocId(filePath);
	const rows = idMap.get(id) ?? [];
	rows.push(path.relative(guidesDir, filePath));
	idMap.set(id, rows);
}

const duplicateRows = [];
for (const [id, rows] of idMap.entries()) {
	if (rows.length > 1) {
		duplicateRows.push(`- '${id}' is defined by: ${rows.join(', ')}`);
	}
}

const missingRows = [];
for (const id of watchedGuideIds) {
	const rows = idMap.get(id) ?? [];
	if (rows.length !== 1) {
		missingRows.push(`- '${id}' expected exactly one source file, found ${rows.length}`);
	}
}

if (duplicateRows.length > 0 || missingRows.length > 0) {
	console.error('Guide content-id validation failed.');
	if (duplicateRows.length > 0) {
		console.error('Duplicate ids:');
		for (const row of duplicateRows) {
			console.error(row);
		}
	}
	if (missingRows.length > 0) {
		console.error('Watchlist id mismatches:');
		for (const row of missingRows) {
			console.error(row);
		}
	}
	process.exit(1);
}

console.log('Guide content-id validation passed.');
