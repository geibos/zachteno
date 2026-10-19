/**
 * Запрет копирования текста теста (настройка `copy_policy`, spec test-taking «Запрет копирования»).
 * Внутри узла отключает выделение, копирование, вырезание, контекстное меню и перетаскивание;
 * поля ввода работают как обычно. Мешает обычному копированию, но не инструментам разработчика.
 */

const BLOCKED = ['copy', 'cut', 'contextmenu', 'dragstart', 'selectstart'] as const;

export type NoCopyParams = { enabled: boolean; onblocked?: () => void };

/** Событие пришло из поля ввода — его не трогаем. */
export function fromField(target: EventTarget | null): boolean {
	const element = target as Partial<Element> | null;
	return element?.closest?.('textarea, input, [contenteditable="true"]') != null;
}

export function noCopy(
	node: HTMLElement,
	params: NoCopyParams
): { update: (next: NoCopyParams) => void; destroy: () => void } {
	let current = params;
	const block = (event: Event) => {
		if (!current.enabled || fromField(event.target)) return;
		event.preventDefault();
		if (event.type === 'copy' || event.type === 'cut') current.onblocked?.();
	};
	const apply = () => {
		node.style.userSelect = current.enabled ? 'none' : '';
	};
	for (const type of BLOCKED) node.addEventListener(type, block);
	apply();
	return {
		update(next) {
			current = next;
			apply();
		},
		destroy() {
			for (const type of BLOCKED) node.removeEventListener(type, block);
			node.style.userSelect = '';
		}
	};
}
