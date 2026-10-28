#!/usr/bin/env node
// Copy docs/*.md into the Starlight collection. Skips docs/site.md (the homepage
// is authored in site/src/content/docs/index.mdx) and the first heading, which
// Starlight prints from frontmatter.

import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const docsDir = path.join(root, 'docs');
const outDir = path.join(root, 'site/src/content/docs');
const repo = 'https://github.com/jonmchan/agent-msg-relay';

function listMarkdown(dir, rel = '') {
	const found = [];
	for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
		if (entry.name.startsWith('.')) continue;
		const relPath = rel ? `${rel}/${entry.name}` : entry.name;
		const full = path.join(dir, entry.name);
		if (entry.isDirectory()) found.push(...listMarkdown(full, relPath));
		else if (entry.name.endsWith('.md') && relPath !== 'site.md') found.push(relPath);
	}
	return found;
}

function outputRel(sourceRel) {
	if (sourceRel.endsWith('/README.md')) return sourceRel.replace(/README\.md$/, 'index.md');
	if (sourceRel === 'README.md') return 'index.md';
	return sourceRel;
}

function yaml(value) {
	return JSON.stringify(value);
}

function descriptionFrom(body) {
	const lines = [];
	let started = false;
	for (const line of body.split('\n')) {
		if (!started) {
			if (line.trim() === '' || line.startsWith('#')) continue;
			started = true;
		}
		if (line.trim() === '') break;
		lines.push(line.trim());
	}
	let text = lines.join(' ');
	text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1').replace(/[`*]/g, '');
	if (text.length > 180) text = `${text.slice(0, 177).trimEnd()}...`;
	return text || 'agent-msg-relay documentation.';
}

function rewriteLinks(markdown, sourceRel) {
	const sourceDir = path.posix.dirname(sourceRel);
	return markdown.replace(/\]\(([^)\s]+)\)/g, (match, href) => {
		if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('#')) return match;
		const hashAt = href.indexOf('#');
		const hrefPath = hashAt === -1 ? href : href.slice(0, hashAt);
		const hash = hashAt === -1 ? '' : href.slice(hashAt);
		if (!hrefPath) return match;
		const fromRepo = path.posix.normalize(path.posix.join('docs', sourceDir, hrefPath));
		if (!fromRepo.startsWith('docs/')) {
			return `](${repo}/blob/main/${fromRepo}${hash})`;
		}
		let next = hrefPath;
		if (next.endsWith('/README.md')) next = next.slice(0, -'README.md'.length);
		else if (next === 'README.md') next = './';
		return `](${next}${hash})`;
	});
}

function page(sourceRel, markdown) {
	const withoutBom = markdown.replace(/^\uFEFF/, '');
	const heading = withoutBom.match(/^#\s+(.+)\s*$/m);
	const title = heading ? heading[1].trim() : sourceRel;
	const body = heading ? withoutBom.replace(/^#\s+.+\n+/, '') : withoutBom;
	const linked = rewriteLinks(body, sourceRel);
	const editPath = sourceRel;
	const hidden = outputRel(sourceRel) === 'adr/index.md' ? '\nsidebar:\n  hidden: true' : '';
	return `---
title: ${yaml(title)}
description: ${yaml(descriptionFrom(linked))}
editUrl: ${yaml(`${repo}/edit/main/docs/${editPath}`)}${hidden}
---

${linked.trimStart()}`;
}

fs.mkdirSync(outDir, { recursive: true });
for (const entry of fs.readdirSync(outDir, { withFileTypes: true })) {
	if (entry.name === 'index.mdx') continue;
	fs.rmSync(path.join(outDir, entry.name), { recursive: true, force: true });
}

for (const sourceRel of listMarkdown(docsDir)) {
	const targetRel = outputRel(sourceRel);
	const target = path.join(outDir, targetRel);
	fs.mkdirSync(path.dirname(target), { recursive: true });
	const markdown = fs.readFileSync(path.join(docsDir, sourceRel), 'utf8');
	fs.writeFileSync(target, page(sourceRel, markdown));
}

console.log(`Synced ${listMarkdown(docsDir).length} docs into site/src/content/docs`);
