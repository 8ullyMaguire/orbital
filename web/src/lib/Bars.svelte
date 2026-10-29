<script>
  let { buckets = [], label = 'clicks', empty = 'Nothing measured yet.' } = $props();
  let max = $derived(Math.max(1, ...buckets.map((b) => b.clicks)));
</script>

{#if buckets.length}
  <table>
    <tbody>
      {#each buckets as b (b.key)}
        <tr>
          <td style="width:38%">{b.key}</td>
          <td>
            <div class="bar"><i style="width:{(b.clicks / max) * 100}%"></i></div>
          </td>
          <td class="num">{b.clicks}</td>
          <td class="num dim xs">{b.visitors} visit{b.visitors === 1 ? '' : 's'}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="muted sm">{empty}</p>
{/if}
