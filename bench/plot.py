#!/usr/bin/env python3
"""Plot the benchmark results against K, comparing the two variants of each
crate with one line per S.

- Defining crate: plain vs announced (rebuild time, `.rmeta` size).
- Calling crate: one chain vs K separate calls (rebuild time, `.rmeta` size).

Reads criterion's estimates (`target/criterion/<group>/<kind>/s<S>_k<K>/new/
estimates.json`, median and its 95% interval) and the size table
(`target/criterion/announce-sizes.csv`) written by `cargo bench`, and writes a
self-contained page `target/criterion/announce-plots.html` (inline SVG, no
dependencies). Usage, from `bench/`: `python3 plot.py [--kind trait] [--svg plots]`;
`--svg DIR` also writes each chart as a standalone SVG (used by the README).
"""
import csv
import html
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CRIT = os.path.join(HERE, "target", "criterion")
KIND = sys.argv[sys.argv.index("--kind") + 1] if "--kind" in sys.argv else "trait"

# (title, unit, [(variant label, data key, hue, dashed)])
CHARTS = [
    ("Defining crate: rebuild time", "ms",
     [("plain", "definer_plain", "blue", True), ("announced", "definer_announce", "blue", False)]),
    ("Defining crate: metadata size (.rmeta)", "KiB",
     [("plain", "plain_rmeta", "blue", True), ("announced", "announce_rmeta", "blue", False)]),
    ("Calling crate: rebuild time", "ms",
     [("K separate calls", "user_calls", "blue", True), ("one chain", "user_chain", "blue", False)]),
    ("Calling crate: metadata size (.rmeta)", "KiB",
     [("K separate calls", "calls_rmeta", "blue", True), ("one chain", "chain_rmeta", "blue", False)]),
]
TIME_GROUPS = ["definer_plain", "definer_announce", "user_chain", "user_calls"]
SIZE_KEYS = ["plain_rmeta", "announce_rmeta", "chain_rmeta", "calls_rmeta"]

# One-hue ordinal ramp, light -> dark = small -> large S; the variant is the line style (validated with the
# dataviz palette validator, --ordinal, against the light and dark surfaces).
RAMPS = {
    "blue": {"light": ["#86b6ef", "#3987e5", "#1c5cab", "#0d366b"],
             "dark": ["#184f95", "#2a78d6", "#6da7ec", "#cde2fb"]},
}


def load():
    data = {}
    for group in TIME_GROUPS:
        d = os.path.join(CRIT, group, KIND)
        if not os.path.isdir(d):
            continue
        for name in os.listdir(d):
            m = re.fullmatch(r"s(\d+)_k(\d+)", name)
            f = os.path.join(d, name, "new", "estimates.json")
            if not m or not os.path.exists(f):
                continue
            med = json.load(open(f))["median"]
            ci = med["confidence_interval"]
            data[(group, int(m.group(1)), int(m.group(2)))] = (
                med["point_estimate"] / 1e6, ci["lower_bound"] / 1e6, ci["upper_bound"] / 1e6)
    for row in csv.DictReader(open(os.path.join(CRIT, "announce-sizes.csv"))):
        if row["kind"] != KIND:
            continue
        for key in SIZE_KEYS:
            v = float(row[key]) / 1024
            data[(key, int(row["S"]), int(row["K"]))] = (v, v, v)
    return data


def log_ticks(lo, hi):
    ticks, e = [], math.floor(math.log10(lo))
    while 10 ** e <= hi * 1.0001:
        ticks += [m * 10 ** e for m in (1, 2, 5) if lo / 1.0001 <= m * 10 ** e <= hi * 1.0001]
        e += 1
    return ticks or [lo, hi]


def fmt(v, unit):
    return f"{v:,.0f} {unit}" if v >= 10 else f"{v:.1f} {unit}"


def chart(title, unit, variants, S, K, data):
    W, H, L, R, T, B = 720, 340, 56, 16, 14, 42
    vals = [data[(key, s, k)][0] for (_, key, _, _) in variants for s in S for k in K if (key, s, k) in data]
    if not vals:
        return ""
    lo, hi = min(vals) / 1.15, max(vals) * 1.15
    xl, xh, yl, yh = math.log(min(K)), math.log(max(K)), math.log(lo), math.log(hi)
    X = lambda x: L + (math.log(x) - xl) / ((xh - xl) or 1) * (W - L - R)
    Y = lambda y: T + (1 - (math.log(y) - yl) / ((yh - yl) or 1)) * (H - T - B)
    g = []
    for t in log_ticks(lo, hi):
        label = f"{t/1000:g}k" if t >= 1000 else f"{t:g}"
        g.append(f'<line class="grid" x1="{L}" x2="{W-R}" y1="{Y(t):.1f}" y2="{Y(t):.1f}"/>'
                 f'<text class="tick" x="{L-6}" y="{Y(t)+3.5:.1f}" text-anchor="end">{label}</text>')
    for k in K:
        g.append(f'<text class="tick" x="{X(k):.1f}" y="{H-B+15}" text-anchor="middle">{k}</text>')
    g.append(f'<line class="axis" x1="{L}" x2="{W-R}" y1="{H-B}" y2="{H-B}"/>'
             f'<text class="axlabel" x="{(L+W-R)/2:.0f}" y="{H-6}" text-anchor="middle">'
             f'K, definitions referenced at once (log scale)</text>'
             f'<text class="axlabel" x="12" y="{T+(H-T-B)/2:.0f}" text-anchor="middle" '
             f'transform="rotate(-90 12 {T+(H-T-B)/2:.0f})">{unit} (log scale)</text>')
    legend = []
    for (vlabel, key, hue, dashed) in variants:
        keys = []
        for i, s in enumerate(S):
            cls = f"{hue}-{i}{' dash' if dashed else ''}"
            line = [(k, data[(key, s, k)]) for k in K if (key, s, k) in data]
            if not line:
                continue
            d = " ".join(f"{'M' if j == 0 else 'L'}{X(k):.1f},{Y(v[0]):.1f}" for j, (k, v) in enumerate(line))
            g.append(f'<path class="line {cls}" d="{d}"/>')
            for k, v in line:
                tip = f"{vlabel}, S = {s}, K = {k}: {fmt(v[0], unit)}"
                if v[1] != v[2]:
                    tip += f" (95% CI {fmt(v[1], unit)} – {fmt(v[2], unit)})"
                g.append(f'<circle class="dot {hue}-{i}" cx="{X(k):.1f}" cy="{Y(v[0]):.1f}" r="4" '
                         f'data-tip="{html.escape(tip)}"><title>{html.escape(tip)}</title></circle>')
            keys.append(f'<span class="key"><svg width="22" height="10"><line class="line {cls}" '
                        f'x1="0" x2="22" y1="5" y2="5"/></svg>S = {s}</span>')
        legend.append(f'<div class="legend"><b>{html.escape(vlabel)}{" (dashed)" if dashed else ""}:</b>{"".join(keys)}</div>')
    return (f'<figure class="chart"><figcaption>{html.escape(title)}</figcaption>{"".join(legend)}'
            f'<svg viewBox="0 0 {W} {H}" role="img" aria-label="{html.escape(title)}">{"".join(g)}</svg></figure>')


SVG_NAMES = ["defining-time", "defining-rmeta", "calling-time", "calling-rmeta"]


def standalone_svg(title, unit, variants, S, K, data):
    """One chart as a self-contained SVG for the README: light theme, inline
    styles, legend inside the image."""
    W, H, L, R, T, B = 720, 380, 56, 16, 74, 42
    ramp = RAMPS["blue"]["light"]
    vals = [data[(key, s, k)][0] for (_, key, _, _) in variants for s in S for k in K if (key, s, k) in data]
    lo, hi = min(vals) / 1.15, max(vals) * 1.15
    xl, xh, yl, yh = math.log(min(K)), math.log(max(K)), math.log(lo), math.log(hi)
    X = lambda x: L + (math.log(x) - xl) / ((xh - xl) or 1) * (W - L - R)
    Y = lambda y: T + (1 - (math.log(y) - yl) / ((yh - yl) or 1)) * (H - T - B)
    font = 'font-family="system-ui,-apple-system,Segoe UI,sans-serif"'
    g = [f'<rect width="{W}" height="{H}" fill="#fcfcfb"/>',
         f'<text x="12" y="20" font-size="14" font-weight="600" fill="#0b0b0b">{html.escape(title)}</text>']
    # Legend: one row per variant, one key per S.
    for r, (vlabel, _, _, dashed) in enumerate(variants):
        y = 40 + r * 16
        g.append(f'<text x="12" y="{y+4}" font-size="11" font-weight="600" fill="#0b0b0b">'
                 f'{html.escape(vlabel)} ({"dashed" if dashed else "solid"})</text>')
        for i, s in enumerate(S):
            x0 = 190 + i * 100
            dash = ' stroke-dasharray="6 4"' if dashed else ""
            g.append(f'<line x1="{x0}" x2="{x0+22}" y1="{y}" y2="{y}" stroke="{ramp[i]}" stroke-width="2"{dash}/>'
                     f'<text x="{x0+28}" y="{y+4}" font-size="11" fill="#52514e">S = {s}</text>')
    for t in log_ticks(lo, hi):
        label = f"{t/1000:g}k" if t >= 1000 else f"{t:g}"
        g.append(f'<line x1="{L}" x2="{W-R}" y1="{Y(t):.1f}" y2="{Y(t):.1f}" stroke="#e6e5e0"/>'
                 f'<text x="{L-6}" y="{Y(t)+3.5:.1f}" font-size="10" fill="#52514e" text-anchor="end">{label}</text>')
    for k in K:
        g.append(f'<text x="{X(k):.1f}" y="{H-B+15}" font-size="10" fill="#52514e" text-anchor="middle">{k}</text>')
    g.append(f'<line x1="{L}" x2="{W-R}" y1="{H-B}" y2="{H-B}" stroke="#52514e"/>'
             f'<text x="{(L+W-R)/2:.0f}" y="{H-6}" font-size="11" fill="#52514e" text-anchor="middle">'
             f'K, definitions referenced at once (log scale)</text>'
             f'<text x="12" y="{T+(H-T-B)/2:.0f}" font-size="11" fill="#52514e" text-anchor="middle" '
             f'transform="rotate(-90 12 {T+(H-T-B)/2:.0f})">{unit} (log scale)</text>')
    for (_, key, _, dashed) in variants:
        for i, s in enumerate(S):
            line = [(k, data[(key, s, k)][0]) for k in K if (key, s, k) in data]
            if not line:
                continue
            d = " ".join(f"{'M' if j == 0 else 'L'}{X(k):.1f},{Y(v):.1f}" for j, (k, v) in enumerate(line))
            dash = ' stroke-dasharray="6 4"' if dashed else ""
            g.append(f'<path d="{d}" fill="none" stroke="{ramp[i]}" stroke-width="2" stroke-linejoin="round"{dash}/>')
            g += [f'<circle cx="{X(k):.1f}" cy="{Y(v):.1f}" r="3.5" fill="{ramp[i]}" stroke="#fcfcfb" stroke-width="1.5"/>'
                  for k, v in line]
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" {font} '
            f'role="img" aria-label="{html.escape(title)}"><title>{html.escape(title)}</title>{"".join(g)}</svg>\n')


def main():
    data = load()
    S = sorted({k[1] for k in data})[:4]
    K = sorted({k[2] for k in data})
    charts = "".join(chart(t, u, v, S, K, data) for (t, u, v) in CHARTS)
    rows = "".join(
        f"<tr><td>{html.escape(t)}</td><td>{html.escape(vl)}</td><td>{s}</td><td>{k}</td><td>{fmt(data[(key, s, k)][0], u)}</td></tr>"
        for (t, u, vs) in CHARTS for (vl, key, _, _) in vs for s in S for k in K if (key, s, k) in data)
    css = []
    for hue, modes in RAMPS.items():
        for i, c in enumerate(modes["light"]):
            css.append(f".{hue}-{i}{{--c:{c}}}")
        for i, c in enumerate(modes["dark"]):
            css.append(f":root[data-theme=dark] .{hue}-{i}{{--c:{c}}}")
            css.append(f"@media (prefers-color-scheme: dark){{:root:not([data-theme=light]) .{hue}-{i}{{--c:{c}}}}}")
    page = f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>announce benchmark plots</title>
<style>
:root{{--surface:#fcfcfb;--ink:#0b0b0b;--ink2:#52514e;--grid:#e6e5e0;color-scheme:light dark}}
@media (prefers-color-scheme: dark){{:root:not([data-theme=light]){{--surface:#1a1a19;--ink:#f0efec;--ink2:#c3c2b7;--grid:#33332f}}}}
:root[data-theme=dark]{{--surface:#1a1a19;--ink:#f0efec;--ink2:#c3c2b7;--grid:#33332f}}
body{{margin:0;padding:16px;background:var(--surface);color:var(--ink);font:14px/1.45 system-ui,sans-serif;max-width:1500px}}
h1{{font-size:20px;margin:0 0 4px}} p.note{{color:var(--ink2);margin:0 0 12px;max-width:900px}}
.grid2{{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,560px),1fr));gap:24px}}
.chart{{margin:0}} figcaption{{font-weight:600;margin-bottom:4px}}
.legend{{display:flex;flex-wrap:wrap;align-items:center;gap:2px 12px;font-size:12px;color:var(--ink2)}}
.legend b{{color:var(--ink);font-weight:600;min-width:150px;white-space:nowrap}}
.key{{display:inline-flex;align-items:center;gap:4px;white-space:nowrap;flex:none}}
svg{{width:100%;height:auto;overflow:visible}}
.grid{{stroke:var(--grid);stroke-width:1}} .axis{{stroke:var(--ink2);stroke-width:1}}
.tick{{fill:var(--ink2);font-size:10px}} .axlabel{{fill:var(--ink2);font-size:11px}}
.line{{fill:none;stroke:var(--c);stroke-width:2;stroke-linejoin:round}} .dash{{stroke-dasharray:6 4}}
.dot{{fill:var(--c);stroke:var(--surface);stroke-width:2}} .dot:hover{{r:6}}
#tip{{position:fixed;pointer-events:none;background:var(--ink);color:var(--surface);padding:4px 8px;border-radius:4px;font-size:12px;display:none;z-index:9}}
details{{margin-top:24px}} table{{border-collapse:collapse;font-size:12px}}
td,th{{padding:2px 8px;border-bottom:1px solid var(--grid);text-align:right}} td:nth-child(-n+2),th:nth-child(-n+2){{text-align:left}}
{chr(10).join(css)}
</style></head><body>
<h1>announce benchmark ({html.escape(KIND)} definitions)</h1>
<p class="note">x axis: K, the number of definitions referenced at once. One line per definition size S (methods per trait); color = S (darker = larger S). Solid = announced / one chain, dashed = plain / K separate calls. Times are criterion medians of crate rebuilds (hover a point for the 95% interval); sizes are the crate's <code>.rmeta</code>. Both axes are logarithmic. The executable is the same size at every point and is not plotted.</p>
<div class="grid2">{charts}</div>
<details><summary>Data table</summary><table><tr><th>chart</th><th>variant</th><th>S</th><th>K</th><th>value</th></tr>{rows}</table></details>
<div id="tip"></div>
<script>
const tip=document.getElementById('tip');
document.querySelectorAll('circle.dot').forEach(c=>{{
  c.addEventListener('mousemove',e=>{{tip.textContent=c.dataset.tip;tip.style.display='block';tip.style.left=(e.clientX+12)+'px';tip.style.top=(e.clientY+12)+'px';}});
  c.addEventListener('mouseleave',()=>tip.style.display='none');
}});
</script></body></html>"""
    if "--svg" in sys.argv:
        d = sys.argv[sys.argv.index("--svg") + 1]
        os.makedirs(d, exist_ok=True)
        for name, (t, u, v) in zip(SVG_NAMES, CHARTS):
            open(os.path.join(d, f"{name}.svg"), "w").write(standalone_svg(t, u, v, S, K, data))
            print(os.path.join(d, f"{name}.svg"))
    out = os.path.join(CRIT, "announce-plots.html")
    open(out, "w").write(page)
    print(out)


if __name__ == "__main__":
    main()
