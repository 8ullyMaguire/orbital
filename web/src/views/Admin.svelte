<script>
  import { api, ago } from '../lib/api.js';

  let { me } = $props();

  let overview = $state(null);
  let flags = $state([]);
  let users = $state([]);
  let err = $state('');
  let ok = $state('');
  let newV = $state({ slug: '', label: '', description: '', icon: 'dot', nsfw: false });

  async function load() {
    err = '';
    try {
      const [o, f, u] = await Promise.all([
        api.get('/api/admin/overview'),
        api.get('/api/admin/flags'),
        api.get('/api/admin/users')
      ]);
      overview = o;
      flags = f.flags;
      users = u.users;
    } catch (e) {
      err = e.message;
    }
  }

  $effect(() => {
    if (me.user.is_owner && !overview) load();
  });

  async function resolve(f, action) {
    try {
      const status = action === 'uphold' ? 'resolved' : 'dismissed';
      await api.post(`/api/admin/flags/${f.id}/resolve`, { status, zero_ranking: action === 'uphold' });
      ok = `Flag ${action === 'uphold' ? 'upheld — ranking zeroed' : 'dismissed'}.`;
      await load();
    } catch (e) {
      err = e.message;
    }
  }

  async function setTrust(u, level) {
    try {
      await api.post(`/api/admin/users/${u.id}`, { trust_level: level, hidden: u.hidden });
      await load();
    } catch (e) {
      err = e.message;
    }
  }

  async function createVertical(e) {
    e.preventDefault();
    err = '';
    try {
      await api.post('/api/admin/verticals', newV);
      ok = `Vertical ${newV.slug} created.`;
      newV = { slug: '', label: '', description: '', icon: 'dot', nsfw: false };
      await load();
    } catch (e) {
      err = e.message;
    }
  }
</script>

{#if err}<p class="msg msg-err">{err}</p>{/if}
{#if ok}<p class="msg msg-ok">{ok}</p>{/if}

{#if !overview}
  <p class="muted sm">Loading…</p>
{:else}
  <div class="tiles">
    <div class="tile"><b>{overview.members}</b><span>members</span></div>
    <div class="tile"><b>{overview.clicks}</b><span>clicks, all time</span></div>
    <div class="tile"><b>{overview.open_flags}</b><span>open flags</span></div>
    <div class="tile"><b>{overview.verticals.length}</b><span>verticals</span></div>
  </div>

  <div class="card" style="margin-top:14px">
    <div class="card-h"><h2>Open flags</h2></div>
    {#if !flags.length}
      <div class="empty">Nothing waiting. Quiet is good.</div>
    {:else}
      <table>
        <thead><tr><th>Target</th><th>Vertical</th><th>Reason</th><th>When</th><th></th></tr></thead>
        <tbody>
          {#each flags as f (f.id)}
            <tr>
              <td><a href="/@{f.target}" target="_blank" rel="noopener">@{f.target}</a></td>
              <td class="xs">{f.vertical || '—'}</td>
              <td class="sm">{f.reason}{#if f.detail}<div class="xs dim">{f.detail}</div>{/if}</td>
              <td class="xs dim">{ago(f.created_at)}</td>
              <td style="white-space:nowrap;text-align:right">
                <button class="btn btn-sm" onclick={() => resolve(f, 'uphold')}>Uphold</button>
                <button class="btn btn-ghost btn-sm" onclick={() => resolve(f, 'dismiss')}>Dismiss</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <div class="card">
    <div class="card-h"><h2>Members</h2></div>
    <table>
      <thead><tr><th>Handle</th><th class="num">Visitors</th><th class="num">Clicks</th><th>Trust</th><th>Joined</th></tr></thead>
      <tbody>
        {#each users as u (u.id)}
          <tr>
            <td><a href="/@{u.handle}" target="_blank" rel="noopener">@{u.handle}</a></td>
            <td class="num">{u.visitors}</td>
            <td class="num">{u.clicks}</td>
            <td>
              <div class="chips">
                {#each [0, 1, 2, 3] as lvl}
                  <button class="chip" class:on={u.trust_level === lvl} onclick={() => setTrust(u, lvl)}>
                    TL{lvl}
                  </button>
                {/each}
              </div>
            </td>
            <td class="xs dim">{ago(u.created_at)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="xs dim" style="margin:10px 0 0">
      Trust is not a mood: TL0 halves a member's trending weight, TL3 adds 10%.
      Demoting someone for manufactured clicks is the whole enforcement mechanism.
    </p>
  </div>

  <div class="card">
    <div class="card-h"><h2>Propose a vertical</h2></div>
    <form onsubmit={createVertical}>
      <div class="row">
        <label>Slug<input bind:value={newV.slug} placeholder="filmmakers" required /></label>
        <label>Label<input bind:value={newV.label} placeholder="Filmmakers" required /></label>
        <label>Icon
          <select bind:value={newV.icon}>
            {#each ['dot', 'film', 'code', 'music', 'mic', 'brush', 'gamepad', 'pen', 'cart', 'rss'] as i}
              <option value={i}>{i}</option>
            {/each}
          </select>
        </label>
        <label style="flex:0 0 auto;flex-direction:row;align-items:center;gap:7px;padding-bottom:9px">
          <input type="checkbox" bind:checked={newV.nsfw} style="width:auto" /> adult
        </label>
        <button class="btn">Create</button>
      </div>
      <label style="margin-top:10px">
        Description
        <input bind:value={newV.description} placeholder="What belongs here, in one line." />
      </label>
    </form>
  </div>
{/if}
