<script>
  import { api } from './lib/api.js';
  import Overview from './views/Overview.svelte';
  import Links from './views/Links.svelte';
  import ProfileForm from './views/ProfileForm.svelte';
  import Standing from './views/Standing.svelte';
  import Embed from './views/Embed.svelte';
  import Admin from './views/Admin.svelte';
  import Auth from './views/Auth.svelte';

  let me = $state(null);
  let standing = $state([]);
  let verticals = $state([]);
  let tab = $state('overview');
  let loading = $state(true);
  let err = $state('');

  async function loadMe() {
    const [m, s] = await Promise.all([api.get('/api/me'), api.get('/api/me/standing')]);
    me = m;
    standing = s.standing;
  }

  async function boot() {
    loading = true;
    try {
      verticals = (await api.get('/api/verticals?all=true')).verticals;
      await loadMe();
    } catch (e) {
      // Only a 401 means "not signed in". Anything else is a broken server, and
      // showing the sign-in form for it invites people to retype a password that
      // was never the problem.
      if (e.status === 401) me = null;
      else err = e.message;
    } finally {
      loading = false;
    }
  }

  boot();

  async function signOut() {
    try {
      await api.post('/api/auth/logout', {});
    } catch {
      // The cookie is what matters; a failed call still clears on the client.
    }
    me = null;
  }

  // Standing and totals are computed server-side at request time, so they must be
  // re-fetched when a tab that displays them opens. A dashboard showing numbers
  // from page load is a dashboard you stop trusting.
  async function pick(key) {
    tab = key;
    if (key === 'overview' || key === 'standing') {
      try {
        await loadMe();
      } catch (e) {
        if (e.status === 401) me = null;
      }
    }
  }

  const tabs = [
    ['overview', 'Overview'],
    ['links', 'Links'],
    ['profile', 'Profile'],
    ['standing', 'Standing'],
    ['embed', 'Embed']
  ];
</script>

{#if loading}
  <div class="shell"><p class="muted sm" style="padding-top:40vh;text-align:center">Loading…</p></div>
{:else if !me}
  {#if err}<p class="msg msg-err" style="max-width:420px;margin:12px auto">{err}</p>{/if}
  <Auth {verticals} onauthed={() => boot()} />
{:else}
  <div class="shell">
    <header class="top">
      <a class="brand" href="/"><span class="orb"></span> Orbital</a>
      <span class="who">@{me.user.handle}</span>
      <span class="spacer"></span>
      <a class="btn btn-ghost btn-sm" href="/@{me.user.handle}" target="_blank" rel="noopener">
        Public page
      </a>
      <button class="btn btn-ghost btn-sm" onclick={signOut}>Sign out</button>
    </header>

    {#if err}<p class="msg msg-err">{err}</p>{/if}

    <nav class="tabs">
      {#each tabs as [key, label]}
        <button class="tab" class:on={tab === key} onclick={() => pick(key)}>{label}</button>
      {/each}
      {#if me.user.is_owner}
        <button class="tab" class:on={tab === 'admin'} onclick={() => pick('admin')}>Admin</button>
      {/if}
    </nav>

    {#if tab === 'overview'}
      <Overview {me} {standing} />
    {:else if tab === 'links'}
      <Links {me} {verticals}/>
    {:else if tab === 'profile'}
      <ProfileForm {me} all={verticals} mine={me.verticals} reload={loadMe} />
    {:else if tab === 'standing'}
      <Standing {standing} {me} />
    {:else if tab === 'embed'}
      <Embed {me} verticals={verticals} />
    {:else if tab === 'admin'}
      <Admin {me} />
    {/if}
  </div>
{/if}
