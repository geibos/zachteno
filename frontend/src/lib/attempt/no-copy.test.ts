import { describe, expect, it } from 'vitest';
import { fromField, noCopy } from './no-copy';

/** Узел без DOM: события и стиль — всё, что нужно action. */
function fakeNode(): HTMLElement {
	return Object.assign(new EventTarget(), { style: { userSelect: '' } }) as unknown as HTMLElement;
}

function fire(node: HTMLElement, type: string): boolean {
	const event = new Event(type, { cancelable: true });
	node.dispatchEvent(event);
	return event.defaultPrevented;
}

describe('noCopy', () => {
	it('при запрете отменяет копирование, вырезание, меню и перетаскивание', () => {
		const node = fakeNode();
		let blocked = 0;
		noCopy(node, { enabled: true, onblocked: () => (blocked += 1) });
		expect(node.style.userSelect).toBe('none');
		for (const type of ['copy', 'cut', 'contextmenu', 'dragstart', 'selectstart']) {
			expect(fire(node, type), type).toBe(true);
		}
		expect(blocked).toBe(2);
	});

	it('без запрета ничего не трогает и снимается обновлением', () => {
		const node = fakeNode();
		const action = noCopy(node, { enabled: true });
		action.update({ enabled: false });
		expect(node.style.userSelect).toBe('');
		expect(fire(node, 'copy')).toBe(false);
	});

	it('после destroy события не отменяются', () => {
		const node = fakeNode();
		noCopy(node, { enabled: true }).destroy();
		expect(fire(node, 'copy')).toBe(false);
		expect(node.style.userSelect).toBe('');
	});
});

describe('fromField', () => {
	it('поле ввода узнаётся по ближайшему предку', () => {
		const inField = { closest: (selector: string) => (selector.includes('textarea') ? {} : null) };
		expect(fromField(inField as unknown as EventTarget)).toBe(true);
		expect(fromField({ closest: () => null } as unknown as EventTarget)).toBe(false);
	});

	it('текстовый узел без closest — не поле', () => {
		expect(fromField({} as EventTarget)).toBe(false);
		expect(fromField(null)).toBe(false);
	});
});
