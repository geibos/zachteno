import MarkdownIt from 'markdown-it';
import katexModule from '@vscode/markdown-it-katex';

// Пакет собран в CommonJS с `exports.default`: в Node (vitest) импорт по умолчанию —
// уже функция, а в браузерной сборке Vite — объект `{ default: fn }`. Приводим к функции.
type KatexPlugin = typeof katexModule;
const katexPlugin: KatexPlugin =
	typeof katexModule === 'function'
		? katexModule
		: (katexModule as unknown as { default: KatexPlugin }).default;

/**
 * Рендер текстов вопросов и вариантов: Markdown + формулы KaTeX (D9).
 * `html: false` — сырой HTML не интерпретируется, а экранируется как текст.
 * Результат безопасно вставлять через `{@html}`.
 */
const md = new MarkdownIt({
	html: false,
	linkify: true,
	breaks: true,
	typographer: false
});

md.use(katexPlugin, {
	throwOnError: false,
	// \href, \url, \htmlClass и т.п. — выключены: только математика.
	trust: false,
	strict: 'ignore',
	output: 'htmlAndMathml'
});

// Ссылки — в новой вкладке, без доступа к window.opener и без Referer.
const defaultLinkOpen =
	md.renderer.rules.link_open ??
	((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));

md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
	const token = tokens[idx];
	if (token) {
		token.attrSet('target', '_blank');
		token.attrSet('rel', 'noopener noreferrer');
	}
	return defaultLinkOpen(tokens, idx, options, env, self);
};

// Внешние картинки запрещены CSP (img-src 'self' data:); пропускаем только свои пути.
const defaultImage =
	md.renderer.rules.image ??
	((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options));

md.renderer.rules.image = (tokens, idx, options, env, self) => {
	const token = tokens[idx];
	const src = String(token?.attrGet('src') ?? '');
	if (!src.startsWith('/') || src.startsWith('//')) {
		return md.utils.escapeHtml(token?.content ?? '');
	}
	token?.attrSet('loading', 'lazy');
	return defaultImage(tokens, idx, options, env, self);
};

/** Markdown + KaTeX → HTML (блочный). */
export function renderMarkdown(source: string): string {
	return md.render(source);
}

/** Markdown + KaTeX → HTML без обёртки в `<p>` (для коротких вариантов ответа). */
export function renderMarkdownInline(source: string): string {
	return md.renderInline(source);
}
