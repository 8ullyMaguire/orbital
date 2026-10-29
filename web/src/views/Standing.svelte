<script>
  import { ordinal } from '../lib/api.js';

  let { standing = [], me } = $props();

  let adult = $derived(standing.some((s) => s.vertical === 'nsfw'));

  function gap(s) {
    if (!s.top_rank) return null;
    if (s.top_rank === 1) return 'you are the board';
    return `${s.top_rank - 1} place${s.top_rank - 1 === 1 ? '' : 's'} to the top`;
  }
</script>

{#if !standing.length}
  <div class="empty">Pick a vertical on the Profile tab and you get a leaderboard.</div>
{:else}
  {#each standing as s (s.vertical)}
    <div class="card">
      <div class="card-h">
        <h2>{s.label}</h2>
        <span class="chip" style="cursor:default">{s.role}</span>
        <span class="spacer"></span>
        <a class="btn btn-ghost btn-sm" href="/v/{s.vertical}" target="_blank" rel="noopener">
          Public board
        </a>
      </div>

      <div class="row" style="align-items:stretch">
        <div class="rank-card" style="flex:1">
          <div class="rank-n">{ordinal(s.top_rank)}<small> of {s.members}</small></div>
          <div class="xs dim">all-time, by clicks</div>
        </div>
        <div class="rank-card" style="flex:1">
          <div class="rank-n">{ordinal(s.trending_rank)}<small> of {s.members}</small></div>
          <div class="xs dim">trending, trailing 7 days</div>
        </div>
        <div class="rank-card" style="flex:1">
          <div class="rank-n">{s.visitors_7d}</div>
          <div class="xs dim">visitors who clicked, 7d</div>
        </div>
      </div>

      <p class="xs dim" style="margin:12px 0 0">
        {#if gap(s)}
          {gap(s)} — trending counts distinct visitors per week, not raw clicks, so
          one person clicking four times moves you once.
        {:else}
          Nobody has clicked through from this vertical yet. The board fills in as
          visitors arrive.
        {/if}
      </p>
    </div>
  {/each}

  {#if adult}
    <div class="card">
      <div class="card-h"><h2>Adult listing</h2></div>
      <p class="sm muted">
        Your adult listing is ranked on the same arithmetic but on its own host,
        behind the age gate, excluded from the sitemap, and never cross-linked to
        your SFW page. Clicks from the adult directory do not count toward your SFW
        standing.
      </p>
    </div>
  {/if}

  <div class="card">
    <div class="card-h"><h2>How the two numbers move</h2></div>
    <table>
      <tbody>
        <tr>
          <td>All-time</td>
          <td class="sm muted">cumulative inbound clicks. No decay: the leaderboard is a monument.</td>
        </tr>
        <tr>
          <td>Trending</td>
          <td class="sm muted">
            clicks in the last 7 days, normalized against the busiest member of that
            vertical, times a trust multiplier and a small new-account boost.
          </td>
        </tr>
        <tr>
          <td>Trust</td>
          <td class="sm muted">
            TL0 ×0.5, TL1 ×0.8, TL2 ×1.0, TL3 ×1.1. New accounts start at TL0 and
            climb by participating.
          </td>
        </tr>
      </tbody>
    </table>
  </div>
{/if}
