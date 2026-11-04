# Freenet analysis for the Gaming Node

Sources: all 95 pages in freenet.org's sitemap (fetched 2026-11-04, no blocking), plus shallow clones of the nine repos in this workspace.

## 1. What Freenet is (per freenet.org)

- A peer-to-peer platform: "a global key-value store" where **keys are WebAssembly contracts** and values are the contract's **state**.
- Peers sit on a **small-world ring** (location 0.0 to 1.0). Routing uses **isotonic regression** on past response times, not plain greedy routing.
- Transport is **FrTP**: UDP, AES-128-GCM, RSA key exchange, NAT hole punching, streaming. The manual says the doc is out of sync with the code.
- Three component types:
  - **Contracts**: public replicated state. They define `validate_state`, `update_state`, `summarize_state` and `get_state_delta`. States must merge commutatively (CRDT-like, eventually consistent).
  - **Delegates**: private agents that hold keys and secrets. They only talk by message and know the sender's identity.
  - **UIs**: web apps served from a contract. They talk to the local node over WebSocket (`/v1/contract/command`, port 7509) via `@freenetorg/freenet-stdlib` (TypeScript) or Rust/Dioxus.
- **Ghost Keys**: anonymous, Sybil-resistant identity certificates, minted by donating. They are meant as the spam and abuse primitive across apps.
- Ecosystem:
  - Shipped: River (chat), Delta (sites), Mail, freenet-git.
  - In development: Harvest, Atlas (RFC stage), Raven.
- Status: **alpha**. Peers auto-update, older versions stop working, and peers send telemetry. Only Windows, macOS, Linux and Docker are supported. There is no mobile app.
- Dev guidance: the Rust toolchain is pinned (1.94.0, with the wasm32 target). There is a `freenet-agent-skills` plugin with a `dapp-builder` skill.

## 2. What this means for the Gaming Node

| Gaming Node phase | Freenet fit | Notes |
|---|---|---|
| Lobby/session discovery (Phase 3) | **Strong** | A lobby list is a small, mergeable, expiring set of signed announcements, which is what a contract is for. Subscriptions give live updates. |
| Spam resistance | **Strong** | Ghost Keys plus the antiflood-token and blind-trust-token examples in the manual map directly onto "signed, expiring, spam-resistant announcements". |
| Key/identity handling | **Strong** | Put node signing keys in a delegate. |
| Gameplay packets (Phase 4) | **Poor as the primary path** | Contract updates are eventually consistent state sync routed over multiple hops. That is the wrong shape for realtime UDP game traffic. Your hybrid plan (Freenet for discovery, direct P2P or volunteer relays for packets) is the right call. |
| NAT traversal (Phase 5) | **Reusable ideas** | FrTP already does hole punching, so there is code to study. It is not a ready-made game transport. |
| Local PC app (Phase 1) | **Independent** | DNS plus auth stub talks to the console, not to Freenet. Phase 1 needs no Freenet, as your own docs say. |

Constraints and risks:
- The node's local API (7509) is fully privileged. The manual says to treat it like SSH. The Gaming Node should talk to it on loopback only, and should not expose it to the console's LAN.
- Alpha status and forced auto-updates mean the contracts and API may break between releases. Pin versions and keep Freenet behind a thin adapter layer.
- Telemetry is on by default. Users should be told that.
- Docs lag the code in places (transport, interface). The cloned source is the real reference.

## 3. Your forks: what each is useful for

| Repo | Files | Use for the Gaming Node |
|---|---|---|
| `freenet-core` | 730 | The node itself. `apps/freenet-ping` is the minimal contract + delegate + UI loop. Start here. `crates/fdev` is the dev tool. `crates/topology-sim` and `crates/replay-harness` are useful for testing. |
| `freenet-stdlib` | 153 | Contract/delegate traits, the Rust and TypeScript client API. This is the dependency for the lobby contract. |
| `river` | 277 | Best model for a "room" contract with membership, signed messages and a delegate. A lobby maps naturally onto a room. Also shows the web-container contract and a CLI (`riverctl`). |
| `freenet-git` | 47 | Secondary. Shows another contract-based app plus Rust crates structure. |
| `mail` | 143 | Secondary. Inbox-style per-user contracts, which is useful if players get private invites. |
| `atlas` | 30 | Secondary. A signed-metadata discovery layer (RFC). Could inform how game lobbies are indexed and searched. |
| `freenet-bitcoin` | 68 | Low relevance. It shows token/bridge contract patterns, which could matter for anti-spam tokens. |
| `web` | 2470 | The freenet.org Hugo source. It is the source of the docs I analyzed, so it is useful for offline reading. Not needed for the node. |

Observations:
- `freenet-gaming-node/reference/` has submodules for RetroArch, emufii, and the **upstream** `freenet/freenet-core` and `freenet/freenet-stdlib`, not your forks. That is fine, but decide which you treat as canonical.
- The README wants RetroArch netplay as a base for "freenet-play", while the Vision docs target console DNS redirection (MK7 on 3DS first). These are two different tracks: emulator netplay, and stock-console DNS. They can share the lobby contract, so the lobby contract is the common piece.

## 4. Suggested baby steps

1. **Phase 0/1, no Freenet**: DNS stub plus MK7 auth stub, tested against Azahar/Citra or a real 3DS. Use Pretendo's open-source server as reference only.
2. **Scaffold a Rust workspace** in `freenet-gaming-node`: `node-core` (console-agnostic traits), `console-3ds`, `freenet-adapter`.
3. **Lobby contract**: model it on `freenet-ping` and River's room contract. State is a set of signed, expiring `LobbyAnnouncement`s, with a merge that keeps the newest per host key and drops expired entries.
4. **Two nodes, one lobby list** on a local Freenet network. `fdev` and `topology-sim` can help here.
5. Then choose a transport (direct P2P, relay, or Freenet) once MK7's real traffic pattern is known.

## 5. Not verified

I did not build or run anything. Clones are shallow (`--depth 1`). I read the main docs pages and repo layouts, not every source file, so deeper claims about internals (for example the exact antiflood token API) should be confirmed against the code before design decisions.
