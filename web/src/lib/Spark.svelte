<script>
  // Two series on one baseline: profile views and outbound clicks. Hand-drawn
  // SVG rather than a charting dependency — 60 lines against 40kb.
  let { series = [], height = 120 } = $props();

  const w = 720;
  let max = $derived(Math.max(1, ...series.map((d) => Math.max(d.views, d.clicks))));
  let n = $derived(Math.max(1, series.length - 1));

  function path(key) {
    if (!series.length) return '';
    return series
      .map((d, i) => {
        const x = (i / n) * w;
        const y = height - (d[key] / max) * (height - 12) - 6;
        return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(' ');
  }

  let area = $derived(
    series.length
      ? `${path('clicks')} L${w},${height} L0,${height} Z`
      : ''
  );
</script>

<div class="spark">
  {#if series.length}
    <svg viewBox="0 0 {w} {height}" preserveAspectRatio="none" role="img"
         aria-label="Daily profile views and outbound clicks">
      <defs>
        <linearGradient id="fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#7c5cff" stop-opacity="0.35" />
          <stop offset="1" stop-color="#7c5cff" stop-opacity="0" />
        </linearGradient>
      </defs>
      <path d={area} fill="url(#fill)" />
      <path d={path('views')} fill="none" stroke="#8d93b0" stroke-width="1.5" stroke-dasharray="4 3" />
      <path d={path('clicks')} fill="none" stroke="#7c5cff" stroke-width="2" />
    </svg>
    <div class="legend">
      <span><i class="k k-clicks"></i> clicks</span>
      <span><i class="k k-views"></i> profile views</span>
    </div>
  {:else}
    <p class="muted sm">No days recorded yet.</p>
  {/if}
</div>

<style>
  .spark svg { width: 100%; height: auto; display: block; }
  .legend { display: flex; gap: 14px; margin-top: 8px; font-size: 0.76rem; color: var(--dim); }
  .k { display: inline-block; width: 10px; height: 2px; margin-right: 5px; vertical-align: middle; }
  .k-clicks { background: #7c5cff; }
  .k-views { background: #8d93b0; }
</style>
