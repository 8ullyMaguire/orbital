<script>
  import { api, fmt } from '../lib/api.js';
  import Spark from '../lib/Spark.svelte';
  import Bars from '../lib/Bars.svelte';

  let { me, standing = [] } = $props();

  let days = $state(30);
  let data = $state(null);
  let err = $state('');
  let loading = $state(true);

  async function load() {
    loading = true;
    err = '';
    try {
      data = await api.get(`/api/analytics?days=${days}`);
    } catch (e) {
      err = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    days;
    load();
  });

  let totals = $derived(data?.totals ?? { views: 0, visitors: 0, clicks: 0, clicks_per_visitor: 0 });
  let primary = $derived(standing.find((s) => s.role === 'primary'));

  // Links opened per visitor. It is a ratio, not a percentage: one visitor who
  // clicks four links is 4.0, and calling that 400% invites the wrong reading.
  let perVisitor = $derived(
    totals.visitors > 0 ? (totals.clicks / totals.visitors).toFixed(1) : '0.0'
  );
</script>

{#if err}<p class="msg msg-err">{err}</p>{/if}

<div class="tiles">
  <div class="tile">
    <b>{fmt(totals.clicks)}</b><span>outbound clicks · {days}d</span>
  </div>
  <div class="tile">
    <b>{fmt(totals.visitors)}</b><span>distinct visitors · {days}d</span>
  </div>
  <div class="tile">
    <b>{fmt(totals.views)}</b><span>profile views · {days}d</span>
  </div>
  <div class="tile">
    <b>{perVisitor}×</b><span>links opened per visitor</span>
  </div>
</div>

{#if primary}
  <div class="card" style="margin-top:14px">
    <div class="card-h">
      <h2>Standing in {primary.label}</h2>
      <span class="spacer"></span>
      <span class="xs dim">your home leaderboard</span>
    </div>
    <div class="rank-card">
      <div class="rank-n">
        {primary.top_rank || '—'}<small> / {primary.members}</small>
      </div>
      <div style="flex:1">
        <div class="sm">
          all-time rank
          {#if primary.trending_rank}
            · <b>#{primary.trending_rank}</b> on trending this week
          {/if}
        </div>
        <div class="xs dim" style="margin-top:3px">
          {primary.visitors_7d} visitor{primary.visitors_7d === 1 ? '' : 's'} clicked through
          in the last 7 days
        </div>
      </div>
      <a class="btn btn-ghost btn-sm" href="/v/{primary.vertical}" target="_blank" rel="noopener">
        Open the board
      </a>
    </div>
  </div>
{/if}

<div class="card" style="margin-top:14px">
  <div class="card-h">
    <h2>Arrivals and exits</h2>
    <span class="spacer"></span>
    <div class="chips">
      {#each [7, 30, 90] as d}
        <button class="chip" class:on={days === d} onclick={() => (days = d)}>{d}d</button>
      {/each}
    </div>
  </div>
  {#if loading}
    <p class="muted sm">Loading…</p>
  {:else}
    <Spark series={data?.series ?? []} />
  {/if}
</div>

<div class="grid2" style="margin-top:14px">
  <div class="card">
    <div class="card-h"><h2>Where they came from</h2></div>
    <Bars buckets={data?.by_surface ?? []} empty="No clicks recorded yet." />
  </div>
  <div class="card">
    <div class="card-h"><h2>Which vertical sent them</h2></div>
    <Bars buckets={data?.by_vertical ?? []} empty="No vertical traffic yet." />
  </div>
</div>

<div class="grid2" style="margin-top:14px">
  <div class="card">
    <div class="card-h"><h2>Which network sent them</h2></div>
    <!-- The surface says which of your pages earned the click; this says whether the Bluesky
         announcement or the Mastodon one actually brought anybody. Host only - no paths. -->
    <Bars buckets={data?.by_referrer ?? []} empty="No referrer recorded yet. Social apps often strip it." />
  </div>
</div>

<div class="card" style="margin-top:14px">
  <div class="card-h">
    <h2>Your links, by pull</h2>
    <span class="spacer"></span>
    <a class="btn btn-ghost btn-sm" href="/@{me.user.handle}" target="_blank" rel="noopener">
      View public page
    </a>
  </div>
  {#if data?.top_links?.length}
    <table>
      <thead>
        <tr><th>Label</th><th>Target</th><th class="num">Clicks</th><th class="num">Visitors</th></tr>
      </thead>
      <tbody>
        {#each data.top_links as l (l.id)}
          <tr>
            <td>{l.label || '(unlabelled)'}</td>
            <td class="dim xs mono" style="max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
              {l.url}
            </td>
            <td class="num">{l.clicks}</td>
            <td class="num dim">{l.visitors}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <div class="empty">
      No clicks yet. Share your profile link — the traffic you get is what the
      leaderboard ranks.
    </div>
  {/if}
</div>
