<script>
  import { api, ago } from '../lib/api.js';

  let data = $state(null);
  let err = $state('');

  async function load() {
    err = '';
    try {
      data = await api.get('/api/search-gaps?limit=50');
    } catch (e) {
      err = e.status === 403
        ? 'Available at TL2 — this queue opens when the site has 50 active contributors.'
        : e.message;
    }
  }
  load();
</script>

{#if err}
  <div class="msg msg-err">{err}</div>
{:else if !data}
  <div class="empty">Loading…</div>
{:else}
  <div class="card">
    <div class="card-h"><h2>Wanted links</h2><span class="chip">{data.gate}</span></div>
    <p class="sm muted">
      URL-shaped searches that found nobody. {data.active_contributors_30d} active contributors in 30 days.
      No visitor is recorded — only the link and how often it was looked for.
    </p>
  </div>
  {#if !data.gaps.length}<div class="empty">Nothing wanted right now.</div>{/if}
  {#each data.gaps as g (g.canonical_key)}
    <div class="card row" style="align-items:center">
      <div style="flex:1;min-width:0">
        <div class="sm" style="word-break:break-all">{g.canonical_key}</div>
        <div class="xs dim">{g.searches} searches · last {ago(g.last_seen)}</div>
      </div>
      <a class="btn btn-sm" href="/app/#/join?url={encodeURIComponent(g.canonical_key.replace(/^url:/, ''))}">
        Add this profile
      </a>
    </div>
  {/each}
{/if}
