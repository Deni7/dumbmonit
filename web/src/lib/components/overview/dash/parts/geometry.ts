/**
 * Pure geometry of the fleet views: the honeycomb (one hexagon per monitor)
 * and the sunburst (site, then category, then monitor). No run-time import,
 * so it runs under `node --test`.
 */

/** Hexagon radius in viewBox units, and how many pixels one unit takes on screen. */
export const HEX_R = 8.6;
export const HEX_SCALE = 1.3;
const HEX_GAP = 1.8;

export interface HexCell {
	cx: number;
	cy: number;
	/** The `points` attribute of the polygon. */
	points: string;
}

export interface HexLayout {
	cells: HexCell[];
	cols: number;
	width: number;
	height: number;
}

/** A pointy-top hexagon centred on (cx, cy). */
export function hexPoints(cx: number, cy: number, r: number = HEX_R): string {
	const points: string[] = [];
	for (let k = 0; k < 6; k++) {
		const a = (Math.PI / 180) * (60 * k - 30);
		points.push(`${(cx + r * Math.cos(a)).toFixed(1)},${(cy + r * Math.sin(a)).toFixed(1)}`);
	}
	return points.join(' ');
}

/** Columns for a site of `count` cells: about twice as wide as tall, 5 to 14, never more than the cells. */
export function hexColumns(count: number): number {
	if (count <= 0) return 1;
	return Math.min(count, Math.max(5, Math.min(14, Math.ceil(Math.sqrt(count * 2)))));
}

/** `count` hexagons in rows of `cols`, every other row shifted by half a cell. */
export function hexGrid(count: number, cols: number = hexColumns(count), r: number = HEX_R): HexLayout {
	const safeCols = Math.max(1, Math.floor(cols));
	const w = Math.sqrt(3) * r;
	const stepX = w + HEX_GAP;
	const stepY = 1.5 * r + HEX_GAP;
	const rows = Math.max(1, Math.ceil(Math.max(0, count) / safeCols));
	const cells: HexCell[] = [];
	for (let i = 0; i < count; i++) {
		const row = Math.floor(i / safeCols);
		const col = i % safeCols;
		const cx = col * stepX + (row % 2 ? stepX / 2 : 0) + w / 2 + 1;
		const cy = row * stepY + r + 1.5;
		cells.push({ cx, cy, points: hexPoints(cx, cy, r) });
	}
	const usedCols = Math.min(safeCols, Math.max(1, count));
	return {
		cells,
		cols: safeCols,
		width: usedCols * stepX + (rows > 1 ? stepX / 2 : 0) + 2,
		height: rows * stepY + r * 0.55 + 3
	};
}

// --- Sunburst ------------------------------------------------------------------

export const TAU = Math.PI * 2;

/** Angle 0 points up, clockwise. */
export function polar(r: number, a: number): [number, number] {
	return [r * Math.sin(a), -r * Math.cos(a)];
}

/** An annular sector between radii r0 < r1 and angles a0 < a1; a whole turn is drawn as two halves. */
export function arcPath(r0: number, r1: number, a0: number, a1: number): string {
	if (!(a1 > a0)) return '';
	if (a1 - a0 >= TAU - 1e-9) {
		return arcPath(r0, r1, a0, a0 + Math.PI) + arcPath(r0, r1, a0 + Math.PI, a0 + TAU - 1e-6);
	}
	const large = a1 - a0 > Math.PI ? 1 : 0;
	const f = (n: number) => n.toFixed(2);
	const [ax, ay] = polar(r1, a0);
	const [bx, by] = polar(r1, a1);
	const [cx, cy] = polar(r0, a1);
	const [dx, dy] = polar(r0, a0);
	return (
		`M${f(ax)} ${f(ay)}A${r1} ${r1} 0 ${large} 1 ${f(bx)} ${f(by)}` +
		`L${f(cx)} ${f(cy)}A${r0} ${r0} 0 ${large} 0 ${f(dx)} ${f(dy)}Z`
	);
}

/** Radii of the three rings and of the leader lines, in viewBox units. */
export const SUN = {
	hole: 50,
	site: [52, 78],
	category: [80, 100],
	monitor: [102, 124],
	leader: [128, 142],
	/** Label band: |y| stays inside it. */
	maxY: 144
} as const;

/** Most monitors named at the edge; the others keep their tooltip. */
export const SUN_LABELS = 8;
const LABEL_GAP = 12;
const PAD = 0.004;

type Status = 'up' | 'down' | 'degraded' | 'paused';

export interface SunMonitor {
	id: number;
	name: string;
	status: Status;
	category: string;
}

export interface SunSite<M extends SunMonitor = SunMonitor> {
	key: string;
	name: string;
	/** Already in category order. */
	monitors: readonly M[];
}

export interface SunArc {
	d: string;
	a0: number;
	a1: number;
}

export interface SunLayout<M extends SunMonitor = SunMonitor> {
	total: number;
	sites: (SunArc & { key: string; name: string; index: number; label: { x: number; y: number } | null })[];
	categories: (SunArc & { site: string; siteIndex: number; category: string; count: number })[];
	monitors: (SunArc & { monitor: M; site: string })[];
	labels: {
		monitor: M;
		x1: number;
		y1: number;
		x2: number;
		y2: number;
		tx: number;
		anchor: 'start' | 'end';
	}[];
}

function padded(a0: number, a1: number, pad: number): [number, number] {
	const p = Math.min(pad, (a1 - a0) * 0.2);
	return [a0 + p, a1 - p];
}

/** Lays out the three rings: each monitor gets the same angle, sites and categories add up. */
export function sunburstLayout<M extends SunMonitor>(input: readonly SunSite<M>[]): SunLayout<M> {
	const total = input.reduce((sum, site) => sum + site.monitors.length, 0);
	const out: SunLayout<M> = { total, sites: [], categories: [], monitors: [], labels: [] };
	if (total === 0) return out;
	const step = TAU / total;
	const single = input.filter((site) => site.monitors.length > 0).length === 1;
	let a = 0;
	const named: { monitor: M; mid: number }[] = [];
	input.forEach((site, index) => {
		const count = site.monitors.length;
		if (count === 0) return;
		const a0 = a;
		const a1 = a + count * step;
		const [s0, s1] = single ? [a0, a1] : padded(a0, a1, PAD * 2);
		const mid = (a0 + a1) / 2;
		const [lx, ly] = polar((SUN.site[0] + SUN.site[1]) / 2, single ? 0 : mid);
		out.sites.push({
			key: site.key,
			name: site.name,
			index,
			d: arcPath(SUN.site[0], SUN.site[1], s0, s1),
			a0,
			a1,
			label: a1 - a0 >= 0.5 ? { x: lx, y: ly + 3 } : null
		});
		let b = a0;
		let i = 0;
		while (i < count) {
			const category = site.monitors[i].category;
			let j = i;
			while (j < count && site.monitors[j].category === category) j++;
			const c0 = b;
			const c1 = b + (j - i) * step;
			const [p0, p1] = single && j - i === count ? [c0, c1] : padded(c0, c1, PAD);
			out.categories.push({ site: site.key, siteIndex: index, category, count: j - i, d: arcPath(SUN.category[0], SUN.category[1], p0, p1), a0: c0, a1: c1 });
			for (let k = i; k < j; k++) {
				const m0 = a0 + k * step;
				const m1 = m0 + step;
				const monitor = site.monitors[k];
				const [q0, q1] = total === 1 ? [m0, m1] : padded(m0, m1, PAD);
				out.monitors.push({ monitor, site: site.key, d: arcPath(SUN.monitor[0], SUN.monitor[1], q0, q1), a0: m0, a1: m1 });
				if (monitor.status === 'down' || monitor.status === 'degraded') named.push({ monitor, mid: (m0 + m1) / 2 });
			}
			b = c1;
			i = j;
		}
		a = a1;
	});
	// Down before degraded, then around the circle; at most SUN_LABELS.
	named.sort((x, y) => rank(x.monitor.status) - rank(y.monitor.status) || x.mid - y.mid);
	const kept = named.slice(0, SUN_LABELS);
	for (const side of [true, false]) {
		const group = kept
			.filter((n) => (Math.sin(n.mid) >= 0) === side)
			.map((n) => {
				const [x1, y1] = polar(SUN.leader[0], n.mid);
				const [x2, y2] = polar(SUN.leader[1], n.mid);
				return { n, x1, y1, x2, y: y2 };
			})
			.sort((p, q) => p.y - q.y);
		spread(group, LABEL_GAP, SUN.maxY);
		for (const p of group) {
			out.labels.push({
				monitor: p.n.monitor,
				x1: p.x1,
				y1: p.y1,
				x2: p.x2,
				y2: p.y,
				tx: p.x2 + (side ? 4 : -4),
				anchor: side ? 'start' : 'end'
			});
		}
	}
	return out;
}

function rank(status: Status): number {
	return status === 'down' ? 0 : status === 'degraded' ? 1 : 2;
}

/** Pushes sorted label positions apart by `gap`, then back inside ±limit. Mutates `y`. */
export function spread(items: { y: number }[], gap: number, limit: number): void {
	for (let i = 1; i < items.length; i++) {
		if (items[i].y < items[i - 1].y + gap) items[i].y = items[i - 1].y + gap;
	}
	const last = items[items.length - 1];
	if (last && last.y > limit) {
		const shift = last.y - limit;
		for (const item of items) item.y -= shift;
	}
	for (let i = 0; i < items.length; i++) {
		const floor = -limit + i * gap;
		if (items[i].y < floor) items[i].y = floor;
	}
}

/** A name cut to `max` characters with an ellipsis. */
export function clip(name: string, max: number): string {
	const chars = [...name];
	return chars.length > max ? `${chars.slice(0, Math.max(1, max - 1)).join('')}…` : name;
}
