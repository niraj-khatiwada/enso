/**
 * Brush strokes, drawn in code: a centerline is swept by a brush whose width follows a profile,
 * with small, seeded irregularities on both edges so every stroke looks hand-made but renders the
 * same on every build.
 */

export type Point = [number, number]

/** A seeded pseudo-random generator (Park–Miller) */
export function seeded(seed: number) {
  let s = seed % 2147483647 || 1
  return () => ((s = (s * 16807) % 2147483647) - 1) / 2147483646
}

/** Low-frequency noise in [-1, 1]: a few sines with seeded phases */
export function wobble(seed: number, octaves = 3) {
  const rnd = seeded(seed)
  const waves = Array.from({ length: octaves }, (_, i) => ({
    f: (i + 1) * (1.6 + rnd() * 1.8),
    p: rnd() * Math.PI * 2,
    a: 1 / (i + 1.4),
  }))
  const total = waves.reduce((sum, w) => sum + w.a, 0)
  return (t: number) => waves.reduce((sum, w) => sum + w.a * Math.sin(t * w.f * Math.PI * 2 + w.p), 0) / total
}

const f = (n: number) => Math.round(n * 100) / 100

export function polyline(points: Point[]): string {
  return points.map(([x, y], i) => `${i ? 'L' : 'M'}${f(x)} ${f(y)}`).join('')
}

export interface BrushOptions {
  /** Width of the stroke at `t` (0 at the start, 1 at the end), in user units */
  width: (t: number) => number
  /** How far the edges wander, in user units */
  roughness?: number
  /** Jagged, blunt start (a brush pressed down) instead of a rounded one */
  bluntStart?: boolean
  seed?: number
}

/** The filled outline of a brush swept along `points` */
export function brush(points: Point[], { width, roughness = 1, bluntStart = false, seed = 1 }: BrushOptions): string {
  const n = points.length - 1
  const left: Point[] = []
  const right: Point[] = []
  const edgeA = wobble(seed * 7 + 1, 4)
  const edgeB = wobble(seed * 13 + 5, 4)
  const fine = seeded(seed * 31 + 3)

  for (let i = 0; i <= n; i++) {
    const t = i / n
    const [x, y] = points[i]
    const [px, py] = points[Math.max(0, i - 1)]
    const [nx, ny] = points[Math.min(n, i + 1)]
    const dx = nx - px
    const dy = ny - py
    const len = Math.hypot(dx, dy) || 1
    const ox = -dy / len
    const oy = dx / len
    const w = width(t) / 2
    // Rougher edges where the brush runs dry (thin), smoother where it's loaded
    const grain = roughness * (0.6 + (1 - Math.min(1, w / 20)) * 0.8)
    const a = w + grain * (edgeA(t * 2.2) * 1.4 + (fine() - 0.5) * 0.5)
    const b = w + grain * (edgeB(t * 2.2) * 1.4 + (fine() - 0.5) * 0.5)
    left.push([x + ox * a, y + oy * a])
    right.push([x - ox * b, y - oy * b])
  }

  const cap: Point[] = []
  if (bluntStart) {
    // A pressed-down start: a few chips across the stroke's first edge
    const [x0, y0] = points[0]
    const [x1, y1] = points[1]
    const len = Math.hypot(x1 - x0, y1 - y0) || 1
    const tx = (x1 - x0) / len
    const ty = (y1 - y0) / len
    const rnd = seeded(seed * 97 + 11)
    const from = right[0]
    const to = left[0]
    for (const s of [0.18, 0.34, 0.5, 0.66, 0.82]) {
      const back = (rnd() - 0.35) * width(0) * 0.22
      cap.push([from[0] + (to[0] - from[0]) * s - tx * back, from[1] + (to[1] - from[1]) * s - ty * back])
    }
  }

  return `${polyline([...left, ...right.reverse(), ...cap])}Z`
}

/** Points along an arc around (cx, cy); angles in degrees, clockwise on screen */
export function arc(cx: number, cy: number, r: (t: number) => number, from: number, sweep: number, steps = 200): Point[] {
  return Array.from({ length: steps + 1 }, (_, i) => {
    const t = i / steps
    const a = ((from + sweep * t) * Math.PI) / 180
    const rr = r(t)
    return [cx + Math.cos(a) * rr, cy + Math.sin(a) * rr]
  })
}

/** Points along a gently bowed line from `a` to `b` */
export function line(a: Point, b: Point, bow = 0, steps = 80): Point[] {
  const dx = b[0] - a[0]
  const dy = b[1] - a[1]
  const len = Math.hypot(dx, dy) || 1
  return Array.from({ length: steps + 1 }, (_, i) => {
    const t = i / steps
    const lift = Math.sin(t * Math.PI) * bow
    return [a[0] + dx * t + (-dy / len) * lift, a[1] + dy * t + (dx / len) * lift]
  })
}

/** The ensō: one breath around the circle, heavy where the brush lands, dry where it lifts */
export function enso(size = 400, seed = 7) {
  const c = size / 2
  const R = size * 0.36
  const W = size * 0.15
  const from = 114
  const sweep = 304
  const drift = wobble(seed, 3)
  const radius = (t: number) => R * (1 + drift(t) * 0.018) - (t > 0.78 ? ((t - 0.78) / 0.22) ** 1.6 * R * 0.06 : 0)
  const width = (t: number) => {
    const load = 0.84 + 0.16 * Math.sin(Math.min(1, t / 0.32) * (Math.PI / 2))
    const lift = t < 0.5 ? 1 : Math.max(0, 1 - ((t - 0.5) / 0.5) ** 1.35)
    return Math.max(0.8, W * load * lift)
  }
  const points = arc(c, c, radius, from, sweep, 260)
  return {
    stroke: brush(points, { width, roughness: size / 260, bluntStart: true, seed }),
    /** The centerline, for revealing the stroke in the order it was drawn */
    path: polyline(points),
    width: W,
    radius: R,
    center: c,
    from,
    sweep,
    /** Dry-brush streaks (kasure) where the ink runs out */
    streaks: [
      { offset: 0.2, start: 0.6, length: 0.22, width: 0.0042 },
      { offset: -0.08, start: 0.66, length: 0.2, width: 0.0028 },
      { offset: 0.05, start: 0.74, length: 0.14, width: 0.0034 },
      { offset: -0.22, start: 0.56, length: 0.12, width: 0.0024 },
    ].map((s) => ({
      d: polyline(arc(c, c, (t) => radius(t) + s.offset * width(s.start + t * s.length), from + sweep * s.start, sweep * s.length, 80)),
      width: size * s.width,
    })),
  }
}
