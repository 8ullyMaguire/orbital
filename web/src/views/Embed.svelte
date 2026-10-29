<script>
  import { copy } from '../lib/api.js';

  let { me, verticals = [] } = $props();

  let view = $state('trending');
  let count = $state(5);
  let slug = $state('dev');
  let host = $state(window.location.origin);
  let copied = $state('');

  // The parent hands us only the verticals this member belongs to; default to
  // their primary once that list exists.
  $effect(() => {
    const own = verticals.find((v) => v.role === 'primary')?.slug;
    if (own && !verticals.some((v) => v.slug === slug)) slug = own;
  });

  // An embed is a funnel, so it must carry the vertical and the surface: the
  // server tags each click with where it was clicked, which is how an embed on
  // someone's blog shows up in their referrer table as `embed`.
  let src = $derived(`${host}/v/${slug}/embed?view=${view}&count=${count}`);
  let iframe = $derived(
    `<iframe src="${src}" width="${count > 5 ? 380 : 300}" height="${count * 52 + 96}" style="border:1px solid #ffffff1f;border-radius:12px;overflow:hidden" frameborder="0" title="Orbital — ${slug} leaderboard" loading="lazy"></iframe>`
  );

  async function copyIt(which) {
    const c = await copy(which === 'iframe' ? iframe : src);
    copied = c ? which : '';
    setTimeout(() => (copied = ''), 1600);
  }
</script>

<div class="card">
  <div class="card-h">
    <h2>Put a leaderboard on your own site</h2>
    <span class="spacer"></span>
    <a class="btn btn-ghost btn-sm" href="/v/{slug}/embed?view={view}&count={count}" target="_blank" rel="noopener">
      Preview
    </a>
  </div>
  <p class="sm muted">
    Every embed is an inbound funnel: a visitor who clicks through from your blog
    arrives already attributed, and their click counts toward whoever's board they
    clicked. A "Join" button sits at the bottom of the widget — that is the whole
    growth loop.
  </p>

  <div class="row" style="margin-top:12px">
    <label>
      Vertical
      <select bind:value={slug}>
        {#each verticals as v (v.slug)}<option value={v.slug}>{v.label}</option>{/each}
      </select>
    </label>
    <label>
      View
      <select bind:value={view}>
        <option value="top">Top — all time</option>
        <option value="trending">Trending — 7 days</option>
        <option value="new">New — recent joins</option>
      </select>
    </label>
    <label>
      Rows
      <select bind:value={count}>
        {#each [3, 5, 8, 10] as n}<option value={n}>{n}</option>{/each}
      </select>
    </label>
    <label>
      Host
      <input bind:value={host} />
    </label>
  </div>

  <h3 class="sm" style="margin:16px 0 6px">Embed</h3>
  <pre>{iframe}</pre>
  <button class="btn btn-ghost btn-sm" style="margin-top:8px" onclick={() => copyIt('iframe')}>
    {copied === 'iframe' ? 'Copied' : 'Copy embed code'}
  </button>

  <h3 class="sm" style="margin:16px 0 6px">Source-tagged direct link</h3>
  <pre>{src}</pre>
  <button class="btn btn-ghost btn-sm" style="margin-top:8px" onclick={() => copyIt('src')}>
    {copied === 'src' ? 'Copied' : 'Copy URL'}
  </button>
</div>

<div class="card">
  <div class="card-h"><h2>Your own tracked address</h2></div>
  <p class="sm muted">
    Use this everywhere: it is the URL that makes every arrival attributable.
    Sharing the raw handle loses the source.
  </p>
  <pre>{host}/@{me.user.handle}?s=link</pre>
</div>
