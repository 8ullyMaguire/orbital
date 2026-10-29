<script>
  import { api } from '../lib/api.js';

  let { verticals = [], onauthed } = $props();

  let mode = $state('login');
  let form = $state({ handle: '', password: '', display_name: '', vertical: 'dev', bio: '' });
  let err = $state('');
  let busy = $state(false);

  let sfw = $derived(verticals.filter((v) => !v.nsfw));

  async function submit(e) {
    e.preventDefault();
    err = '';
    busy = true;
    try {
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
    <h1>{mode === 'login' ? 'Sign in' : 'Join Orbital'}</h1>
    <p class="sm muted">
      {mode === 'login'
        ? 'Your dashboard: links, traffic, and where you rank.'
        : 'Pick the vertical you belong to — that is the leaderboard you start on.'}
    </p>

    <div class="switch">
      <button class:on={mode === 'login'} onclick={() => (mode = 'login')}>Sign in</button>
      <button class:on={mode === 'register'} onclick={() => (mode = 'register')}>Create account</button>
    </div>

    {#if err}<p class="msg msg-err">{err}</p>{/if}

    <form onsubmit={submit}>
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
        {busy ? 'Working…' : mode === 'login' ? 'Sign in' : 'Create my profile'}
      </button>
    </form>

    <p class="xs dim" style="margin:14px 0 0">
      Password, not a social login: no third party sits between you and your page.
      Adult listings live on a separate host and are never linked from this one.
    </p>
  </div>
</div>
