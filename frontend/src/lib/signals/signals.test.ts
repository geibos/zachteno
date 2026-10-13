import { describe, expect, it } from 'vitest';
import { ensureMarker, generateMarker, isMarker, type MarkerStore } from './marker';
import { isPasswordField, observeTyping, TypingRecorder, MAX_SAMPLES } from './typing';
import { normalizeGpuString, parseOs } from './fingerprint';

function memoryStore(name: string, initial: string | null = null, broken = false): MarkerStore & { value: string | null } {
	return {
		name,
		value: initial,
		async read() {
			if (broken) throw new Error('недоступно');
			return this.value;
		},
		async write(value: string) {
			if (broken) throw new Error('недоступно');
			this.value = value;
		}
	};
}

describe('маркер', () => {
	it('128 бит в hex', () => {
		const marker = generateMarker();
		expect(marker).toMatch(/^[0-9a-f]{32}$/);
		expect(generateMarker()).not.toBe(marker);
	});

	it('восстанавливается из единственного уцелевшего хранилища и дописывается в остальные', async () => {
		const survivor = 'a'.repeat(32);
		const cookie = memoryStore('cookie');
		const local = memoryStore('localStorage');
		const idb = memoryStore('indexedDB', survivor);
		const marker = await ensureMarker([cookie, local, idb], () => 'b'.repeat(32));
		expect(marker).toBe(survivor);
		expect(cookie.value).toBe(survivor);
		expect(local.value).toBe(survivor);
	});

	it('битое значение не считается маркером, сломанное хранилище не мешает', async () => {
		const cookie = memoryStore('cookie', 'испорчено');
		const broken = memoryStore('localStorage', null, true);
		const idb = memoryStore('indexedDB');
		const marker = await ensureMarker([cookie, broken, idb], () => 'c'.repeat(32));
		expect(marker).toBe('c'.repeat(32));
		expect(cookie.value).toBe(marker);
		expect(idb.value).toBe(marker);
		expect(isMarker('испорчено')).toBe(false);
	});
});

/** Поле-заглушка без DOM: EventTarget с атрибутами, события приходят с target = поле. */
class FakeField extends EventTarget {
	constructor(
		public type: string,
		public id = ''
	) {
		super();
	}
}

function key(kind: 'keydown' | 'keyup', code: string, keyValue: string): Event {
	const event = new Event(kind);
	Object.defineProperty(event, 'code', { value: code });
	Object.defineProperty(event, 'key', { value: keyValue });
	return event;
}

function typeWord(field: EventTarget, word: string, clock: { t: number }): void {
	for (const char of word) {
		const code = `Key${char.toUpperCase()}`;
		field.dispatchEvent(key('keydown', code, char));
		clock.t += 80;
		field.dispatchEvent(key('keyup', code, char));
		clock.t += 120;
	}
}

describe('ритм набора', () => {
	it('в снимке только интервалы — без кодов и символов клавиш', () => {
		const recorder = new TypingRecorder();
		const email = new FakeField('email', 'email');
		const clock = { t: 0 };
		const stop = observeTyping(email, recorder, () => true, () => clock.t);
		typeWord(email, 'qwerty', clock);
		stop();
		const snapshot = recorder.snapshot();
		expect(snapshot).not.toBeNull();
		expect(snapshot!.dwell).toEqual([80, 80, 80, 80, 80, 80]);
		expect(snapshot!.flight).toEqual([120, 120, 120, 120, 120]);
		expect(snapshot!.input).toBe('keyboard');
		expect(Object.keys(snapshot!).sort()).toEqual(['dwell', 'flight', 'input']);
		const json = JSON.stringify(snapshot);
		expect(json).not.toMatch(/Key[A-Z]/);
		expect(json).not.toMatch(/"[qwerty]"/);
	});

	it('поле пароля не порождает событий, даже если его пропускает фильтр', () => {
		const recorder = new TypingRecorder();
		const password = new FakeField('password', 'password');
		const clock = { t: 0 };
		observeTyping(password, recorder, () => true, () => clock.t);
		typeWord(password, 'секретныйпароль', clock);
		expect(recorder.size).toBe(0);
		expect(recorder.snapshot()).toBeNull();
	});

	it('autocomplete=*-password тоже считается паролем', () => {
		expect(isPasswordField({ type: 'text', autocomplete: 'new-password' })).toBe(true);
		expect(isPasswordField({ type: 'email', autocomplete: 'email' })).toBe(false);
	});

	it('не больше 256 значений', () => {
		const recorder = new TypingRecorder();
		for (let i = 0; i < 400; i += 1) {
			recorder.keyDown('k', i * 10);
			recorder.keyUp('k', i * 10 + 5);
		}
		const snapshot = recorder.snapshot()!;
		expect(snapshot.dwell.length).toBe(MAX_SAMPLES);
		expect(snapshot.flight.length).toBe(MAX_SAMPLES);
	});

	it('касание помечает ввод как touch', () => {
		const recorder = new TypingRecorder();
		recorder.markTouch();
		recorder.keyDown('a', 0);
		recorder.keyUp('a', 50);
		expect(recorder.snapshot()?.input).toBe('touch');
	});
});

describe('признаки устройства', () => {
	it('нормализует WebGL renderer без версий драйвера', () => {
		expect(
			normalizeGpuString('ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 (0x00002504) Direct3D11 vs_5_0 ps_5_0, D3D11-31.0.15.3623)')
		).toBe('angle (nvidia, nvidia geforce rtx 3060 direct3d11)');
		expect(normalizeGpuString('Mesa Intel(R) UHD Graphics 620 (KBL GT2) Mesa 23.2.1')).toBe(
			'mesa intel(r) uhd graphics 620 (kbl gt2) mesa'
		);
	});

	it('ОС по User-Agent', () => {
		expect(parseOs('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toEqual({ os: 'Windows', version: '10.0' });
		expect(parseOs('Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X)')).toEqual({ os: 'iOS', version: '17.4' });
		expect(parseOs('Mozilla/5.0 (Linux; Android 14; Pixel 8)').os).toBe('Android');
	});
});
