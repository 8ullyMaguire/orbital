<script>
  import { api } from '../lib/api.js';

  let { verticals = [], onauthed } = $props();

  // The public site links to /app/#/join; honour it instead of dropping the visitor on sign-in.
  let mode = $state(location.hash === '#/join' ? 'contribute' : 'login');
  let form = $state({ handle: '', password: '', display_name: '', vertical: 'dev', bio: '', url: '' });
  let err = $state('');
  let busy = $state(false);
  let added = $state(null);

  let sfw = $derived(verticals.filter((v) => !v.nsfw));
  let ordered = $derived([...sfw, ...verticals.filter((v) => v.nsfw)]);

  async function submit(e) {
    e.preventDefault();
    err = '';
    busy = true;
    try {
      if (mode === 'contribute') {
        // No account: the URL is the identity. A repeat is a lookup, not a duplicate profile.
        added = await api.post('/api/contribute/profile', {
          url: form.url.trim(),
          display_name: form.display_name.trim(),
          vertical: form.vertical,
          bio: form.bio
        });
        return;
      }
      const body =
        mode === 'login'
          ? { handle: form.handle.trim().toLowerCase(), password: form.password }
          : {
              handle: form.handle.trim().toLowerCase(),
              password: form.password,
              display_name: form.display_name || form.handle,
              vertical: form.vertical,
              bio: form.bio
            };
      const res = await api.post(mode === 'login' ? '/api/auth/login' : '/api/auth/register', body);
      onauthed(res.user);
    } catch (e) {
      err = e.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="auth">
  <div class="card">
    {#if added}
      <h1>{added.already_existing ? 'Already here' : 'Profile added'}</h1>
      <p class="sm muted">
        {#if added.already_existing}
          That link already has a profile. We found it instead of making a second one.
        {:else}
          It is live now. Add more links from your dashboard once you claim it.
        {/if}
      </p>
      <p class="msg msg-ok">
        <a href="/@{added.handle}" target="_blank" rel="noopener">@{added.handle}</a>
        — {added.display_name}
      </p>
      <button class="btn btn-ghost" style="width:100%;margin-top:10px" onclick={() => { added = null; mode = 'login'; }}>
        Sign in to manage it
      </button>
    {:else}
    <h1>{mode === 'login' ? 'Sign in' : mode === 'register' ? 'Join Orbital' : 'Add a profile'}</h1>
    <p class="sm muted">
      {mode === 'login'
        ? 'Your dashboard: links, traffic, and where you rank.'
        : mode === 'register'
          ? 'Pick the vertical you belong to — that is the leaderboard you start on.'
          : 'Paste a link-in-bio page. No account needed while the site is young.'}
    </p>

    <div class="switch">
      <button class:on={mode === 'login'} onclick={() => (mode = 'login')}>Sign in</button>
      <button class:on={mode === 'register'} onclick={() => (mode = 'register')}>Create account</button>
      <button class:on={mode === 'contribute'} onclick={() => (mode = 'contribute')}>Add a profile</button>
    </div>

    {#if err}<p class="msg msg-err">{err}</p>{/if}

    <form onsubmit={submit}>
      {#if mode === 'contribute'}
        <label>
          Link
          <input
            type="url"
            bind:value={form.url}
            placeholder="https://allmylinks.com/you"
            autocomplete="url"
            required
          />
        </label>
        <label style="margin-top:10px">
          Display name
          <input bind:value={form.display_name} placeholder="Your Name" maxlength="60" />
        </label>
        <label style="margin-top:10px">
          Vertical
          <select bind:value={form.vertical}>
            {#each ordered as v (v.slug)}<option value={v.slug}>{v.label}{v.nsfw ? ' (adult)' : ''}</option>{/each}
          </select>
        </label>
        <label style="margin-top:10px">
          One line about you
          <input bind:value={form.bio} maxlength="600" placeholder="What you make, in a sentence." />
        </label>
      {:else}
      <label>
        Handle
        <input bind:value={form.handle} placeholder="yourname" autocomplete="username" required />
      </label>
      <label style="margin-top:10px">
        Password
        <input
          type="password"
          bind:value={form.password}
          autocomplete={mode === 'login' ? 'current-password' : 'new-password'}
          minlength="10"
          required
        />
      </label>
      {/if}

      {#if mode === 'register'}
        <label style="margin-top:10px">
          Display name
          <input bind:value={form.display_name} placeholder="Your Name" maxlength="60" />
        </label>
        <label style="margin-top:10px">
          Vertical
          <select bind:value={form.vertical}>
            {#each sfw as v (v.slug)}<option value={v.slug}>{v.label}</option>{/each}
          </select>
        </label>
        <label style="margin-top:10px">
          One line about you
          <input bind:value={form.bio} maxlength="600" placeholder="What you make, in a sentence." />
        </label>
      {/if}

      <button class="btn" style="width:100%;margin-top:14px" disabled={busy}>
        {busy ? 'Working…' : mode === 'login' ? 'Sign in' : mode === 'register' ? 'Create my profile' : 'Add it'}
      </button>
    </form>

    <p class="xs dim" style="margin:14px 0 0">
      Password, not a social login: no third party sits between you and your page.
      Adult listings live on a separate host and are never linked from this one.
    </p>
    {/if}
  </div>
</div>
