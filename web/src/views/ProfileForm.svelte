<script>
  import { api } from '../lib/api.js';

  let { me, all = [], mine = [], reload } = $props();

  let handle = $derived(me.user.handle);
  let draft = $state({
    display_name: me.user.display_name,
    bio: me.user.bio,
    avatar_url: me.user.avatar_url ?? '',
    accent: me.user.accent ?? '#7c5cff',
    tags: (me.tags ?? []).join(', '),
    primary_vertical:
      mine.find((v) => v.role === 'primary')?.slug ?? mine[0]?.slug ?? 'dev',
    secondary_verticals: mine.filter((v) => v.role === 'secondary').map((v) => v.slug)
  });
  let err = $state('');
  let ok = $state('');
  let busy = $state(false);

  let adultNudge = $derived(draft.primary_vertical === 'nsfw' || draft.secondary_verticals.includes('nsfw'));

  function toggleSecondary(slug) {
    if (slug === draft.primary_vertical) return;
    if (draft.secondary_verticals.includes(slug)) {
      draft.secondary_verticals = draft.secondary_verticals.filter((s) => s !== slug);
    } else if (draft.secondary_verticals.length < 2) {
      draft.secondary_verticals = [...draft.secondary_verticals, slug];
    }
  }

  // Picking a new primary drops it from the secondary list rather than sending a
  // duplicate the server would silently drop.
  function setPrimary(slug) {
    draft.primary_vertical = slug;
    draft.secondary_verticals = draft.secondary_verticals.filter((s) => s !== slug);
  }

  async function save() {
    busy = true;
    err = '';
    ok = '';
    try {
      await api.patch('/api/me', {
        display_name: draft.display_name,
        bio: draft.bio,
        avatar_url: draft.avatar_url,
        accent: draft.accent,
        tags: draft.tags
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean),
        primary_vertical: draft.primary_vertical,
        secondary_verticals: draft.secondary_verticals
      });
      ok = 'Saved.';
      await reload();
    } catch (e) {
      err = e.message;
    } finally {
      busy = false;
    }
  }
</script>

{#if err}<p class="msg msg-err">{err}</p>{/if}
{#if ok}<p class="msg msg-ok">{ok}</p>{/if}

<div class="card">
  <div class="card-h">
    <h2>The card strangers see</h2>
    <span class="spacer"></span>
    <a class="btn btn-ghost btn-sm" href="/@{handle}" target="_blank" rel="noopener">Open it</a>
  </div>

  <div class="grid2">
    <label>
      Display name
      <input bind:value={draft.display_name} maxlength="60" />
    </label>
    <label>
      Handle <span class="dim xs">(permanent, it is your URL)</span>
      <input value={handle} disabled />
    </label>
  </div>

  <label style="margin-top:12px">
    Bio
    <textarea bind:value={draft.bio} rows="4" maxlength="600"></textarea>
  </label>

  <div class="grid2" style="margin-top:12px">
    <label>
      Avatar URL <span class="dim xs">(blank = a generated mark)</span>
      <input bind:value={draft.avatar_url} placeholder="https://…" />
    </label>
    <label>
      Accent colour
      <input bind:value={draft.accent} placeholder="#7c5cff" />
    </label>
  </div>

  <label style="margin-top:12px">
    Taste tags <span class="dim xs">(comma separated, up to 10 — these drive the recommendations)</span>
    <input bind:value={draft.tags} placeholder="rust, self-hosting, fanfiction" />
  </label>
</div>

<div class="card">
  <div class="card-h">
    <h2>Your vertical</h2>
    <span class="spacer"></span>
    <span class="xs dim">one primary, up to two secondaries (ranked at 0.7×)</span>
  </div>

  <h3 class="sm" style="margin-bottom:8px">Primary — the leaderboard you live on</h3>
  <div class="chips">
    {#each all as v (v.slug)}
      <button class="chip" class:on={draft.primary_vertical === v.slug} onclick={() => setPrimary(v.slug)}>
        {v.label}{#if v.nsfw}<span class="dim xs"> · adult</span>{/if}
      </button>
    {/each}
  </div>

  <h3 class="sm" style="margin:16px 0 8px">
    Secondary — you appear here too, with less weight
  </h3>
  <div class="chips">
    {#each all as v (v.slug)}
      {@const isPrimary = draft.primary_vertical === v.slug}
      {@const on = draft.secondary_verticals.includes(v.slug)}
      {@const full = !on && draft.secondary_verticals.length >= 2}
      <button class="chip" class:on disabled={isPrimary || full} onclick={() => toggleSecondary(v.slug)}>
        {v.label}
      </button>
    {/each}
  </div>

  {#if adultNudge}
    <p class="xs" style="margin:14px 0 0;color:var(--warn)">
      Your profile is listed under Adult Creators. That listing lives on the
      separate adult host, is gated, is not indexed, and is never linked from
      your SFW page.
    </p>
  {/if}

  <div class="row" style="margin-top:16px">
    <button class="btn" onclick={save} disabled={busy}>{busy ? 'Saving…' : 'Save changes'}</button>
    <span class="xs dim">Ranking recalculates on the next page load, not on a schedule.</span>
  </div>
</div>

<div class="card">
  <div class="card-h"><h2>Bluesky verification</h2></div>
  <p class="sm muted">
    Prove the handle is yours by posting this code from your Bluesky account, then
    claim it from the API. A verified handle puts a ✓ on your card — the cheapest
    trust signal a directory can offer.
  </p>
  <pre>orbital-verify:{me.verify_token}</pre>
</div>
