/**
 * Признаки браузера и устройства для антифрода (design D14).
 * Только локальные замеры: без сторонних библиотек отпечатков и без внешних запросов.
 * Каждый замер в try/catch — недоступный API даёт пропуск компонента, а не ошибку.
 * Сервер хранит только HMAC от значений; лимиты: ≤64 компонентов, значение ≤2048 символов.
 */
import { FONT_LIST } from './fonts';

export type Components = Record<string, string>;

const MAX_VALUE = 2048;

function put(target: Components, key: string, value: unknown): void {
	if (value === undefined || value === null || value === '') return;
	const text = String(value);
	target[key] = text.length > MAX_VALUE ? text.slice(0, MAX_VALUE) : text;
}

async function attempt<T>(fn: () => T | Promise<T>): Promise<T | undefined> {
	try {
		return await fn();
	} catch {
		return undefined;
	}
}

/** SHA-256 → hex. */
export async function sha256Hex(data: string | ArrayBuffer | Uint8Array): Promise<string> {
	const bytes = typeof data === 'string' ? new TextEncoder().encode(data) : data;
	const digest = await crypto.subtle.digest('SHA-256', bytes as BufferSource);
	return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

/**
 * Нормализация строки WebGL renderer/vendor: нижний регистр, без версий драйверов
 * и служебных хвостов в скобках, чтобы обновление драйвера не меняло признак.
 */
export function normalizeGpuString(value: string): string {
	return value
		.toLowerCase()
		.replace(/\(0x[0-9a-f]+\)/g, ' ') // (0x00002504)
		.replace(/,\s*d3d\d+(-[\d.]+)?/g, ' ') // , D3D11-31.0.15.3623
		.replace(/\b(vs|ps)_\d+_\d+\b/g, ' ') // vs_5_0 ps_5_0
		.replace(/\b\d+(\.\d+){1,}\b/g, ' ') // версии вида 31.0.15.3623, 4.6.0
		.replace(/\(\s*\)/g, ' ')
		.replace(/\s*,\s*(?=[,)])/g, '')
		.replace(/\s+/g, ' ')
		.replace(/\(\s+/g, '(')
		.replace(/\s+\)/g, ')')
		.trim();
}

/** ОС и версия по User-Agent (запасной путь, если нет Client Hints). */
export function parseOs(ua: string): { os: string; version: string } {
	const pick = (re: RegExp) => re.exec(ua)?.[1]?.replace(/_/g, '.') ?? '';
	if (/Windows NT/.test(ua)) return { os: 'Windows', version: pick(/Windows NT ([\d.]+)/) };
	if (/iPhone|iPad|iPod/.test(ua)) return { os: 'iOS', version: pick(/OS (\d+[_\d]*)/) };
	if (/Android/.test(ua)) return { os: 'Android', version: pick(/Android ([\d.]+)/) };
	if (/CrOS/.test(ua)) return { os: 'ChromeOS', version: pick(/CrOS \S+ ([\d.]+)/) };
	if (/Mac OS X/.test(ua)) return { os: 'macOS', version: pick(/Mac OS X (\d+[_.\d]*)/) };
	if (/Linux/.test(ua)) return { os: 'Linux', version: '' };
	return { os: 'unknown', version: '' };
}

// --- браузер ---

async function canvasHash(): Promise<string | undefined> {
	const canvas = document.createElement('canvas');
	canvas.width = 280;
	canvas.height = 60;
	const ctx = canvas.getContext('2d');
	if (!ctx) return undefined;
	ctx.textBaseline = 'alphabetic';
	ctx.fillStyle = '#f60';
	ctx.fillRect(100, 1, 62, 20);
	ctx.fillStyle = '#069';
	ctx.font = '14px Arial';
	ctx.fillText('Зачтено, ∫x dx = ½ 😀', 2, 15);
	ctx.fillStyle = 'rgba(102, 204, 0, 0.7)';
	ctx.font = '18px Times New Roman';
	ctx.fillText('Sphinx of black quartz', 4, 45);
	ctx.beginPath();
	ctx.arc(240, 30, 20, 0, Math.PI * 2);
	ctx.stroke();
	return sha256Hex(canvas.toDataURL());
}

async function audioHash(): Promise<string | undefined> {
	const Ctx =
		(globalThis as { OfflineAudioContext?: typeof OfflineAudioContext }).OfflineAudioContext ??
		(globalThis as { webkitOfflineAudioContext?: typeof OfflineAudioContext }).webkitOfflineAudioContext;
	if (!Ctx) return undefined;
	const context = new Ctx(1, 5000, 44100);
	const oscillator = context.createOscillator();
	oscillator.type = 'triangle';
	oscillator.frequency.value = 10000;
	const compressor = context.createDynamicsCompressor();
	compressor.threshold.value = -50;
	compressor.knee.value = 40;
	compressor.ratio.value = 12;
	compressor.attack.value = 0;
	compressor.release.value = 0.25;
	oscillator.connect(compressor);
	compressor.connect(context.destination);
	oscillator.start(0);
	const rendered = await Promise.race([
		context.startRendering(),
		new Promise<never>((_, reject) => setTimeout(() => reject(new Error('audio timeout')), 1000))
	]);
	const samples = rendered.getChannelData(0).slice(4500, 5000);
	let sum = 0;
	for (const sample of samples) sum += Math.abs(sample);
	return sha256Hex(sum.toFixed(10));
}

/** Какие API есть у браузера: строка из отсортированных имён. */
function availableApis(): string {
	const g = globalThis as Record<string, unknown>;
	const nav = navigator as unknown as Record<string, unknown>;
	const checks: Array<[string, boolean]> = [
		['serviceWorker', 'serviceWorker' in navigator],
		['bluetooth', 'bluetooth' in nav],
		['usb', 'usb' in nav],
		['hid', 'hid' in nav],
		['serial', 'serial' in nav],
		['xr', 'xr' in nav],
		['share', 'share' in nav],
		['clipboard', 'clipboard' in nav],
		['credentials', 'credentials' in nav],
		['gpu', 'gpu' in nav],
		['locks', 'locks' in nav],
		['mediaSession', 'mediaSession' in nav],
		['wakeLock', 'wakeLock' in nav],
		['userAgentData', 'userAgentData' in nav],
		['storageBuckets', 'storageBuckets' in nav],
		['PaymentRequest', 'PaymentRequest' in g],
		['Notification', 'Notification' in g],
		['SharedArrayBuffer', 'SharedArrayBuffer' in g],
		['OffscreenCanvas', 'OffscreenCanvas' in g],
		['BroadcastChannel', 'BroadcastChannel' in g],
		['WebTransport', 'WebTransport' in g],
		['EyeDropper', 'EyeDropper' in g],
		['speechSynthesis', 'speechSynthesis' in g],
		['webkitSpeechRecognition', 'webkitSpeechRecognition' in g],
		['documentPictureInPicture', 'documentPictureInPicture' in g],
		['showOpenFilePicker', 'showOpenFilePicker' in g],
		['ontouchstart', 'ontouchstart' in g],
		['pdfViewerEnabled', Boolean(nav.pdfViewerEnabled)],
		['cookieEnabled', navigator.cookieEnabled]
	];
	return checks
		.filter(([, present]) => present)
		.map(([name]) => name)
		.sort()
		.join(',');
}

export async function collectBrowser(): Promise<Components> {
	const result: Components = {};
	put(result, 'ua', navigator.userAgent);
	put(result, 'canvas', await attempt(canvasHash));
	put(result, 'audio', await attempt(audioHash));
	put(result, 'apis', await attempt(availableApis));
	put(result, 'do_not_track', navigator.doNotTrack ?? '');
	// Зависят от браузера, а не от железа: в отпечаток устройства их класть нельзя,
	// иначе один компьютер в Chromium и Firefox даёт разные отпечатки.
	put(result, 'languages', (navigator.languages ?? [navigator.language]).join(','));
	put(result, 'memory', (navigator as { deviceMemory?: number }).deviceMemory);
	const gl = await attempt(webglInfo);
	if (gl) {
		put(result, 'webgl_max_viewport', gl.maxViewport);
		if (gl.extensions) {
			put(result, 'webgl_extensions', await attempt(() => sha256Hex(gl.extensions!.join(','))));
			put(result, 'webgl_extensions_count', gl.extensions.length);
		}
	}
	const os = await osInfo();
	put(result, 'os_version', os.version);
	const media = await attempt(mediaCounts);
	if (media) for (const [kind, n] of Object.entries(media)) put(result, `media_${kind}`, n);
	return result;
}

// --- устройство ---

interface WebGlInfo {
	renderer?: string;
	vendor?: string;
	maxTexture?: number;
	maxViewport?: string;
	extensions?: string[];
}

function webglInfo(): WebGlInfo | undefined {
	const canvas = document.createElement('canvas');
	const gl = (canvas.getContext('webgl') ?? canvas.getContext('experimental-webgl')) as WebGLRenderingContext | null;
	if (!gl) return undefined;
	const info: WebGlInfo = {};
	const debug = gl.getExtension('WEBGL_debug_renderer_info');
	if (debug) {
		info.renderer = String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) ?? '');
		info.vendor = String(gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) ?? '');
	}
	info.maxTexture = Number(gl.getParameter(gl.MAX_TEXTURE_SIZE));
	const dims = gl.getParameter(gl.MAX_VIEWPORT_DIMS) as Int32Array | null;
	if (dims) info.maxViewport = `${dims[0]}x${dims[1]}`;
	info.extensions = [...(gl.getSupportedExtensions() ?? [])].sort();
	gl.getExtension('WEBGL_lose_context')?.loseContext();
	return info;
}

/** Битовая маска найденных шрифтов (hex) по индексам FONT_LIST и их число. */
function detectFonts(): { mask: string; count: number } | undefined {
	const body = document.body;
	if (!body) return undefined;
	const probe = 'mmmmmmmmmmlli1WwQ@#ЖщЫ';
	const bases = ['monospace', 'sans-serif', 'serif'];
	const span = document.createElement('span');
	span.textContent = probe;
	span.setAttribute('aria-hidden', 'true');
	span.style.cssText =
		'position:absolute;left:-9999px;top:-9999px;font-size:72px;line-height:normal;white-space:nowrap;visibility:hidden';
	body.appendChild(span);
	try {
		const baseSize = new Map<string, string>();
		for (const base of bases) {
			span.style.fontFamily = base;
			baseSize.set(base, `${span.offsetWidth}x${span.offsetHeight}`);
		}
		const bits = new Uint8Array(Math.ceil(FONT_LIST.length / 8));
		let count = 0;
		FONT_LIST.forEach((font, index) => {
			const found = bases.some((base) => {
				span.style.fontFamily = `"${font}", ${base}`;
				return `${span.offsetWidth}x${span.offsetHeight}` !== baseSize.get(base);
			});
			if (found) {
				bits[index >> 3]! |= 1 << (index & 7);
				count += 1;
			}
		});
		return { mask: Array.from(bits, (b) => b.toString(16).padStart(2, '0')).join(''), count };
	} finally {
		span.remove();
	}
}

async function osInfo(): Promise<{ os: string; version: string }> {
	const uaData = (navigator as { userAgentData?: { getHighEntropyValues?: (hints: string[]) => Promise<Record<string, string>> } }).userAgentData;
	if (uaData?.getHighEntropyValues) {
		const values = await attempt(() => uaData.getHighEntropyValues!(['platform', 'platformVersion']));
		if (values?.platform) return { os: values.platform, version: values.platformVersion ?? '' };
	}
	return parseOs(navigator.userAgent);
}

async function mediaCounts(): Promise<Record<string, number> | undefined> {
	if (!navigator.mediaDevices?.enumerateDevices) return undefined;
	// Без запроса разрешений: браузер отдаёт только типы (без имён).
	const devices = await navigator.mediaDevices.enumerateDevices();
	const counts: Record<string, number> = { audioinput: 0, audiooutput: 0, videoinput: 0 };
	for (const device of devices) counts[device.kind] = (counts[device.kind] ?? 0) + 1;
	return counts;
}

export async function collectDevice(): Promise<Components> {
	const result: Components = {};
	const gl = await attempt(webglInfo);
	if (gl) {
		put(result, 'webgl_renderer', gl.renderer ? normalizeGpuString(gl.renderer) : undefined);
		put(result, 'webgl_vendor', gl.vendor ? normalizeGpuString(gl.vendor) : undefined);
		put(result, 'webgl_max_texture', gl.maxTexture);
	}
	put(result, 'screen', `${screen.width}x${screen.height}`);
	put(result, 'color_depth', screen.colorDepth);
	put(result, 'pixel_ratio', window.devicePixelRatio);
	put(result, 'cores', navigator.hardwareConcurrency);
	put(result, 'touch_points', navigator.maxTouchPoints);
	const fonts = await attempt(detectFonts);
	if (fonts) {
		put(result, 'fonts', fonts.mask);
		put(result, 'fonts_count', fonts.count);
	}
	put(result, 'timezone', await attempt(() => Intl.DateTimeFormat().resolvedOptions().timeZone));
	// Client Hints пишут «Chrome OS», разбор UA — «ChromeOS»: одно устройство, одно имя.
	put(result, 'os', (await osInfo()).os.replace(/\s+/g, ''));
	return result;
}
