# Tauri + SvelteKit + TypeScript

This template should help get you started developing with Tauri, SvelteKit and TypeScript in Vite.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).


## Updater
export TAURI_SIGNING_PRIVATE_KEY="/Users/paolo/.tauri/teleporter2.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="xxx"
then build, get sig content and copy it in server.json
then update version matching tauri.conf.json