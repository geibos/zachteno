import { describe, expect, it } from 'vitest';
import { renderMarkdown, renderMarkdownInline } from './markdown';

describe('renderMarkdown', () => {
	it('экранирует <script>, а не выводит его как разметку', () => {
		const html = renderMarkdown('Вопрос <script>alert(1)</script> конец');
		expect(html).not.toMatch(/<script/i);
		expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
	});

	it('не пропускает HTML-атрибуты с обработчиками событий', () => {
		const html = renderMarkdown('<img src=x onerror="alert(1)">');
		expect(html).not.toMatch(/<img/i);
		expect(html).toContain('&lt;img');
	});

	it('не создаёт javascript:-ссылок', () => {
		const html = renderMarkdown('[жми](javascript:alert(1))');
		expect(html).not.toMatch(/href="javascript:/i);
	});

	it('рендерит inline-формулу KaTeX', () => {
		const html = renderMarkdown('Найдите $x^2 + 1 = 0$');
		expect(html).toContain('class="katex"');
		expect(html).toContain('<math');
		expect(html).not.toContain('$x^2');
	});

	it('рендерит блочную формулу KaTeX', () => {
		const html = renderMarkdown('$$\n\\int_0^1 x\\,dx = \\frac{1}{2}\n$$');
		expect(html).toContain('katex-display');
	});

	it('не исполняет \\href и прочие небезопасные команды KaTeX', () => {
		const html = renderMarkdown('$\\href{javascript:alert(1)}{x}$');
		expect(html).not.toMatch(/href="javascript:/i);
	});

	it('ошибочная формула не роняет рендер', () => {
		expect(() => renderMarkdown('$\\frac{1}{$')).not.toThrow();
	});

	it('ссылки открываются с rel="noopener noreferrer"', () => {
		const html = renderMarkdown('[сайт](https://example.org)');
		expect(html).toContain('rel="noopener noreferrer"');
		expect(html).toContain('target="_blank"');
	});

	it('пропускает только локальные картинки', () => {
		expect(renderMarkdown('![схема](/api/files/abc)')).toContain('<img src="/api/files/abc"');
		expect(renderMarkdown('![трекер](https://evil.example/p.gif)')).not.toMatch(/<img/i);
	});

	it('inline-режим не оборачивает в <p>', () => {
		expect(renderMarkdownInline('**да**')).toBe('<strong>да</strong>');
	});
});
