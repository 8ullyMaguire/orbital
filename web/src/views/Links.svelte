<script>
  import { api, copy } from '../lib/api.js';

  let { me, verticals = [], reload } = $props();

  let links = $state([]);
  let err = $state('');
  let ok = $state('');
  let busy = $state(false);
  let draft = $state({ url: '', label: '', icon: 'link', vertical_slug: '' });
  let editing = $state(null);

  // Every member of a vertical can scope a link to it: the same profile can show
  // a different button set inside #gaming than it does on the front door.
  let scopes = $derived([{ slug: '', label: 'Everywhere' }, ...verticals.map((v) => ({ slug: v.slug, label: v.label }))]);

  async function load() {
    try {
      const d = await api.get('/api/links');
      links = d.links;
    } catch (e) {
      err = e.message;
    }
  }

  $effect(() => {
    links;
    if (!links.length) load();
  });

  async function add(e) {
    e.preventDefault();
    busy = true;
    err = '';
    ok = '';
    try {
      await api.post('/api/links', {
        url: draft.url.trim(),
        label: draft.label.trim(),
        icon: draft.icon || 'link',
        vertical_slug: draft.vertical_slug || null
      });
      draft = { url: '', label: '', icon: 'link', vertical_slug: '' };
      ok = 'Link added. It is live on your page now.';
      await load();
    } catch (e) {
      err = e.message;
    } finally {
      busy = false;
    }
  }

  async function save(l) {
    err = '';
    try {
      await api.patch(`/api/links/${l.id}`, {
        label: l.label,
        url: l.url,
        icon: l.icon,
        vertical_slug: l.vertical_slug || null,
        hidden: l.hidden
      });
      editing = null;
      await load();
    } catch (e) {
      err = e.message;
    }
  }

  async function remove(l) {
    if (!confirm(`Delete “${l.label || l.url}”? Its clicks stay in your analytics.`)) return;
    try {
      await api.del(`/api/links/${l.id}`);
      await load();
    } catch (e) {
      err = e.message;
    }
  }

  async function move(i, dir) {
    const j = i + dir;
    if (j < 0 || j >= links.length) return;
    const order = links.map((l) => l.id);
    [order[i], order[j]] = [order[j], order[i]];
    const snapshot = links;
    links = order.map((id) => snapshot.find((l) => l.id === id));
    try {
      await api.post('/api/links/reorder', { order });
      await load();
    } catch (e) {
      err = e.message;
      links = snapshot;
    }
  }

  let copied = $state('');
  async function copyTracked(l) {
    const c = await copy(l.tracked);
    copied = c ? l.id : '';
    setTimeout(() => (copied = ''), 1500);
  }
</script>

{#if err}<p class="msg msg-err">{err}</p>{/if}
{#if ok}<p class="msg msg-ok">{ok}</p>{/if}

<div class="card">
  <div class="card-h"><h2>Add a link</h2></div>
  <form onsubmit={add}>
    <div class="row">
      <label style="flex:2 1 260px">
        URL
        <input bind:value={draft.url} placeholder="https://example.com" required />
      </label>
      <label style="flex:1 1 160px">
        Label
        <input bind:value={draft.label} placeholder="My portfolio" maxlength="60" />
      </label>
      <label style="flex:1 1 130px">
        Icon
        <select bind:value={draft.icon}>
          <option value="link">link</option>
          <option value="code">code</option>
          <option value="film">video</option>
          <option value="music">music</option>
          <option value="mic">podcast</option>
          <option value="brush">art</option>
          <option value="gamepad">game</option>
          <option value="pen">writing</option>
          <option value="cart">shop</option>
          <option value="mail">contact</option>
          <option value="rss">feed</option>
        </select>
      </label>
      <label style="flex:1 1 140px">
        Show on
        <select bind:value={draft.vertical_slug}>
          {#each scopes as s}<option value={s.slug}>{s.label}</option>{/each}
        </select>
      </label>
      <button class="btn" disabled={busy}>{busy ? 'Adding…' : 'Add link'}</button>
    </div>
  </form>
  <p class="xs dim" style="margin:10px 0 0">
    Scoping a link to a vertical only shows it when a visitor arrives from that
    vertical's leaderboard — useful for a different set of buttons per audience.
  </p>
</div>

<div class="card">
  <div class="card-h">
    <h2>Your links</h2>
    <span class="spacer"></span>
    <span class="xs dim">{links.length} total</span>
  </div>

  {#if !links.length}
    <div class="empty">Nothing yet. Add the first link above.</div>
  {:else}
    <table>
      <thead>
        <tr><th></th><th>Label</th><th>Target</th><th>Scope</th><th class="num">Clicks</th><th></th></tr>
      </thead>
      <tbody>
        {#each links as l, i (l.id)}
          <tr>
            <td style="width:34px">
              <div style="display:flex;flex-direction:column;gap:2px">
                <button class="btn-ghost btn-xs" style="padding:0 4px;border:0;background:none;cursor:pointer"
                  onclick={() => move(i, -1)} title="Move up">↑</button>
                <button class="btn-ghost" style="padding:0 4px;border:0;background:none;cursor:pointer"
                  onclick={() => move(i, 1)} title="Move down">↓</button>
              </div>
            </td>
            {#if editing === l.id}
              <td><input bind:value={l.label} placeholder="Label" maxlength="60" /></td>
              <td><input bind:value={l.url} placeholder="https://" /></td>
              <td>
                <select bind:value={l.vertical_slug}>
                  {#each scopes as s}<option value={s.slug}>{s.label}</option>{/each}
                </select>
              </td>
              <td class="num">{l.click_count}</td>
              <td style="white-space:nowrap">
                <button class="btn btn-sm" onclick={() => save(l)}>Save</button>
                <button class="btn btn-ghost btn-sm" onclick={() => (editing = null)}>Cancel</button>
              </td>
            {:else}
              <td>
                {#if l.label}{l.label}{:else}<span class="dim">(unlabelled)</span>{/if}
                {#if l.failing}
                  <div class="xs" style="color:#ffb4a2">
                    {l.last_status ? `HTTP ${l.last_status}` : 'no answer'} on the last {l.fail_count} checks
                  </div>
                {/if}
              </td>
              <td class="dim xs mono" style="max-width:240px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                {l.url}
              </td>
              <td class="xs">{l.vertical_slug || 'everywhere'}</td>
              <td class="num">{l.click_count}</td>
              <td style="white-space:nowrap;text-align:right">
                <button class="btn btn-ghost btn-sm" onclick={() => (editing = l.id)}>Edit</button>
                <button class="btn btn-ghost btn-sm" onclick={() => copyTracked(l)}>
                  {copied === l.id ? 'Copied' : 'Copy link'}
                </button>
                <button class="btn btn-danger btn-sm" onclick={() => remove(l)}>Delete</button>
              </td>
            {/if}
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
