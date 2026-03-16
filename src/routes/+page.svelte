<script lang="ts">
  import { onMount } from "svelte";
  import CableIcon from "@lucide/svelte/icons/cable";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleCheckBigIcon from "@lucide/svelte/icons/circle-check-big";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import PlugZapIcon from "@lucide/svelte/icons/plug-zap";
  import { ArrowRight } from "@lucide/svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button/index.js";
  import SunIcon from "@lucide/svelte/icons/sun";
  import MoonIcon from "@lucide/svelte/icons/moon";
  import { toggleMode } from "mode-watcher";

  type TunnelStatus = "pending" | "active" | "stopped" | "error";

  type TunnelRuntime = {
    local: number;
    name: string;
    dest: string;
    remote: number;
    status: TunnelStatus;
    message: string;
  };

  type TunnelStatePayload = {
    connected: boolean;
    tunnels: TunnelRuntime[];
  };

  let connected = $state(false);
  let pending = $state(false);
  let statusMsg = $state("");
  let tunnels = $state<TunnelRuntime[]>([]);

  function applyState(payload: TunnelStatePayload) {
    connected = payload.connected;
    tunnels = payload.tunnels;
  }

  async function refreshState() {
    try {
      connected = await invoke("tunnel_status");
      tunnels = (await invoke("tunnel_list")) as TunnelRuntime[];
    } catch (error) {
      statusMsg = String(error);
    }
  }

  onMount(() => {
    void refreshState();

    let unlisten: (() => void) | undefined;

    void listen<TunnelStatePayload>("tunnel-state", (event) => {
      applyState(event.payload);
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      unlisten?.();
    };
  });

  function badgeClasses(status: TunnelStatus) {
    if (status === "active")
      return "bg-emerald-500/12 text-emerald-700 dark:text-emerald-300";
    if (status === "error") return "bg-destructive/12 text-destructive";
    if (status === "stopped") return "bg-secondary text-secondary-foreground";
    return "bg-amber-500/12 text-amber-700 dark:text-amber-300";
  }

  function statusLabel(status: TunnelStatus) {
    if (status === "active") return "Active";
    if (status === "error") return "Error";
    if (status === "stopped") return "Stopped";
    return "Pending";
  }

  async function toggleConnection(event: Event) {
    event.preventDefault();

    if (pending) {
      return;
    }

    pending = true;
    statusMsg = connected
      ? await invoke("disconnect_tunnel")
      : await invoke("connect_tunnel");
    await refreshState();
    pending = false;
  }
</script>

<svelte:head>
  <title>Teleporter</title>
</svelte:head>

<main
  class="relative min-h-screen overflow-hidden bg-background px-6 py-10 text-foreground"
>
  <div class="mx-auto flex w-full max-w-4xl flex-col gap-6">
    <section
      class="rounded-[28px] border border-border/70 bg-card/85 p-6 shadow-[0_24px_80px_-32px_rgba(15,23,42,0.35)] backdrop-blur"
    >
      <div
        class="flex flex-col gap-6 md:flex-row md:items-end md:justify-between"
      >
        <div
          class={`inline-flex items-center gap-2 rounded-full px-3 py-1 text-xs font-medium ${connected ? "bg-emerald-500/12 text-emerald-700 dark:text-emerald-300" : "bg-secondary text-secondary-foreground"}`}
        >
          <CableIcon class="size-3.5" />
          Teleporter
        </div>

        <Button onclick={toggleMode} variant="outline" size="icon">
          <SunIcon
            class="h-[1.2rem] w-[1.2rem] scale-100 rotate-0 !transition-all dark:scale-0 dark:-rotate-90"
          />
          <MoonIcon
            class="absolute h-[1.2rem] w-[1.2rem] scale-0 rotate-90 !transition-all dark:scale-100 dark:rotate-0"
          />
          <span class="sr-only">Toggle theme</span>
        </Button>

        <form
          class="flex flex-col items-start gap-3 md:items-end"
          onsubmit={toggleConnection}
        >
          <Button
            type="submit"
            size="lg"
            class="min-w-40 rounded-full px-6"
            disabled={pending}
          >
            <PlugZapIcon class="size-4" />
            {#if pending}
              {connected ? "Disconnecting..." : "Connecting..."}
            {:else}
              {connected ? "Disconnect" : "Connect"}
            {/if}
          </Button>
        </form>
      </div>
    </section>

    <section
      class="rounded-[28px] border border-border/70 bg-card/80 p-6 backdrop-blur"
    >
      <div class="mb-4 flex items-center justify-between gap-3">
        <div>
          <h2 class="text-lg font-semibold tracking-tight">Tunnel plan</h2>
          <p class="text-sm text-muted-foreground">
            Each item maps a local port to a remote service name.
          </p>
        </div>
        <div
          class="rounded-full border border-border/70 px-3 py-1 text-xs text-muted-foreground"
        >
          {tunnels.length} tunnel{tunnels.length === 1 ? "" : "s"}
        </div>
      </div>

      {#if tunnels.length > 0}
        <div class="grid gap-3 sm:grid-cols-2">
          {#each tunnels as tunnel}
            <article
              class="rounded-2xl border border-border/70 bg-background/85 p-4 shadow-sm"
            >
              <div class="flex items-start justify-between">
                <div
                  class="rounded-full bg-secondary px-2.5 py-1 text-xs text-secondary-foreground"
                >
                  localhost:{tunnel.local}
                </div>

                <ArrowRight class="text-neutral-400" />

                <div
                  class="rounded-full bg-secondary px-2.5 py-1 text-xs text-secondary-foreground"
                >
                  {tunnel.name}
                </div>
              </div>
            </article>
          {/each}
        </div>
      {:else}
        <div
          class="rounded-2xl border border-dashed border-border/80 bg-background/60 px-4 py-10 text-center text-sm text-muted-foreground"
        >
          Connect to load the stub tunnel plan.
        </div>
      {/if}
    </section>
  </div>
</main>
