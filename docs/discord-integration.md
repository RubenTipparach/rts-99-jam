# Discord integration - setup

This game uses Discord for **sign-in**, for **finding your friends** (share a room
code in chat, or hop in via a Discord invite), and - for the developer - for an
**admin portal** gated to your Discord account. It can also post **match
notifications** (a friend opened a room, your match is starting) either as DMs to
opted-in players or to a channel webhook.

There are two levels of setup:

- **Login + admin (recommended):** full OAuth - players click **"Sign in with
  Discord,"** and your Discord ID unlocks the admin dashboard. Needs the client
  ID/secret below.
- **Notifications only (minimal):** just a bot token (DMs) or a channel webhook,
  with no login.

> Flavor note for the portal/app description: this is *Astromancers* - a
> deterministic RTS of **space and magic**, where the wizard nation of the
> Astromancers and the tech-driven magicless fight across the solar system. Use that as the
> Discord application name/description so the consent screen reads on-brand.

---

## 1. Create the bot (once)

In the [Discord Developer Portal](https://discord.com/developers/applications):

1. **New Application** → name it (e.g. `Astromancers`). The name + icon here are
   what players see on the OAuth consent screen, so upload the game emblem
   (`assets/branding/emblem.png`).
2. **Bot** (left nav) → **Add Bot**.
3. **Reset Token** → copy it. This is your `DISCORD_BOT_TOKEN` - treat it like a
   password (see [§3](#3-give-the-server-the-secrets)).
4. **Privileged Gateway Intents:** leave all **off**. Login, room codes, and
   basic DMs need no privileged intents.

That token alone is enough for the **notifications-only** path. For sign-in,
continue to §2.

---

## 2. Set up "Sign in with Discord" (OAuth)

### 2a. (Optional) Make the bot's guild

Only needed if you want **guild-gated features** (e.g. "this room is open to
members of our Discord," or auto-adding players to your server).

1. Create an empty private Discord server.
2. Invite the bot: Dev Portal → **OAuth2 → URL Generator** → scope `bot` → open
   the generated URL → add it to your server.
3. Enable **Developer Mode** (Discord → Settings → Advanced), right-click the
   server → **Copy Server ID** → that's `DISCORD_GUILD_ID`.

Skip this entirely if you only want login + room codes.

### 2b. Get the OAuth client credentials

Dev Portal → **OAuth2**:

- Copy **Client ID** → `DISCORD_CLIENT_ID`.
- **Reset Secret** → copy → `DISCORD_CLIENT_SECRET`.

### 2c. Register the redirect URL

Still under **OAuth2 → Redirects**, add your callback URL(s):

```
https://rts-99-jam.fly.dev/auth/discord/callback
http://localhost:8080/auth/discord/callback
```

- Replace `rts-99-jam` with your actual Fly app name.
- Add the `localhost` line for local development.
- You usually **don't** need to set `DISCORD_REDIRECT_URI` - the server derives
  this callback automatically. Set it (§3) only if OAuth fails behind the proxy;
  a URL mismatch is the #1 OAuth error.

**Scopes we request:** `identify` (who you are) and, only if you set up §2a,
`guilds.join`. We never ask for message-reading or email scopes.

---

## 3. Give the server the secrets

The server is configured through the environment variables below - set them with
`fly secrets set`. There's nothing else to configure: the SQLite file lives at a
built-in default path on the mounted Fly Volume (see below).

| Variable | Required for | Why it's needed / where it comes from |
|---|---|---|
| `DISCORD_CLIENT_ID` | Sign-in | Public ID of your Discord app. Dev Portal → OAuth2. |
| `DISCORD_CLIENT_SECRET` | Sign-in | Proves the server *is* your app when exchanging the OAuth code. Dev Portal → OAuth2 → Reset Secret. **Secret.** |
| `DISCORD_BOT_TOKEN` | Bot DMs / presence / invites | Authenticates the bot user. Dev Portal → Bot → Reset Token. **Secret.** Optional if you only do login. |
| `DISCORD_GUILD_ID` | Guild-gated features | ID of your bot's server (§2a). Optional. |
| `ADMIN_DISCORD_ID` | Admin portal | Your Discord **user** ID - unlocks the admin dashboard for that account. Developer Mode → right-click your name → Copy User ID. |
| `FLY_API_TOKEN` | CI auto-deploy | Deploy token for the GitHub Actions workflow that pushes to Fly. Lives in **GitHub repo secrets**, not `fly secrets`. Create with `fly tokens create deploy`. |
| `DISCORD_REDIRECT_URI` | Sign-in (only if needed) | Usually **not set** - the server derives the callback URL. Add it only if OAuth fails behind the proxy (then use the exact §2c URL). |

### Why a database at all if we're on Fly?

Fly runs the container but **does not keep its filesystem** across deploys or
restarts - so users, match records, and the AoE2-style stat charts need storage
that survives. We use **SQLite on a Fly Volume** - the same pattern as
high-frontier-fan-game:

```
fly volumes create data --size 1
```

Mount it at `/data` in `fly.toml`; the server opens its SQLite file there by
default. That's the whole story - nothing to configure: no `DATABASE_URL`, no
`DATABASE_PATH`, no separate database service.

### Sign-in needs no signing secret

Sessions are **opaque**: on login the server generates a random token, stores it
in a `sessions` row in that same SQLite file, and sets it as an httpOnly cookie.
Each request is authenticated by a DB lookup, not by verifying a signature - so
there is **no `SESSION_SECRET`/JWT key** to set or rotate. (The OAuth `state`
value is likewise a random token checked server-side.) Admin access is just: look
up the session → get its `discord_id` → compare to `ADMIN_DISCORD_ID`.

### Set the secrets on Fly

```bash
fly secrets set \
  DISCORD_CLIENT_ID=... \
  DISCORD_CLIENT_SECRET=... \
  DISCORD_BOT_TOKEN=... \
  DISCORD_GUILD_ID=... \
  ADMIN_DISCORD_ID=YOUR_DISCORD_USER_ID
```

The database needs no config - the server opens SQLite on the mounted volume by
default. `FLY_API_TOKEN` is a **GitHub** repo secret for the deploy workflow
(`fly tokens create deploy`), not something you set with `fly secrets`.

> ⚠️ Never paste real tokens/secrets into the repo, a commit, or chat. They live
> only in `fly secrets` (production) and your local `.env` (git-ignored). The repo
> ships only `.env.example` with placeholders.

### Local development

```bash
# .env (git-ignored) - placeholders shown
DISCORD_CLIENT_ID=...
DISCORD_CLIENT_SECRET=...
ADMIN_DISCORD_ID=YOUR_DISCORD_USER_ID
# DISCORD_REDIRECT_URI=http://localhost:8080/auth/discord/callback  # only if auto-detect fails

cargo run -p server      # apps/server (axum); reads the vars above
```

Features light up progressively as variables become available: with **no** env
vars the server runs and serves the lobby on its default SQLite file; add the
`DISCORD_CLIENT_*` pair to enable sign-in; add `ADMIN_DISCORD_ID` to unlock the
admin portal; add `DISCORD_BOT_TOKEN` for DMs/presence.

---

## 4. How players sign in (in the app)

From the main menu → **Sign in with Discord**:

1. The system browser opens the Discord consent screen (`identify` scope).
2. Approve → Discord redirects to `/auth/discord/callback` with a one-time code.
3. The server exchanges it for the player's identity, creates an **opaque session**
   (random token in SQLite, httpOnly cookie), and stores `{ discord_id, username,
   avatar }`.
4. The player is back in the lobby, now able to **create rooms**, **join by
   6-character code**, and appear to friends.

**Finding friends:** share your room's 6-character code in any Discord channel or
DM - a friend pastes it into **Join by code**. (Deeper "Join via Discord" using
Rich Presence invites is a planned enhancement.)

**Admin:** if your `discord_id` matches `ADMIN_DISCORD_ID`, the menu also shows
**Admin dashboard** → games created, players per room, players authed, game
length, scores, and live in-match charts (buildings / war units / utility units /
resources over time).

---

## What triggers a Discord message

Opt-in, per player. One event = one message, no throttling:

| Event | Message |
|---|---|
| A friend opens a room | "🛰️ `username` opened a room - code `AB12CD`" |
| Your room fills / match starts | "⚔️ Your match is starting - `N` nation-corps deployed" |
| Match ends | "🏁 Match over - winner: `username` (`duration`)" |

Delivered as **DMs** to players who linked Discord and toggled notifications on,
or to a **channel webhook** (below) for a whole community.

---

## Channel webhook (no bot)

The simplest path for a community feed - no per-player linking:

1. In a Discord channel → **Edit Channel → Integrations → Webhooks → New
   Webhook** → copy the URL.
2. Provide it via the admin dashboard, or set `DISCORD_WEBHOOK_URL` as a Fly
   secret.
3. Clear the field to disable.

Posts the same events as above, once per channel instead of per-DM. Treat the
webhook URL as a secret.

---

## Privacy / security notes

- We store only your Discord **user ID**, username, and avatar hash - opt-in, and
  deletable from your profile.
- `DISCORD_CLIENT_SECRET` and `DISCORD_BOT_TOKEN` are **server-side only**; they
  never reach the game client.
- OAuth requests the minimum scopes (`identify`, plus `guilds.join` only if you
  enabled §2a); the access token is single-use and not persisted.
- Sessions are opaque random tokens stored server-side (SQLite); deleting the
  `sessions` row logs that session out. No signing key to manage or leak.
- Admin access is strictly the `ADMIN_DISCORD_ID` allowlist - there is no
  password to leak.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `redirect_uri_mismatch` | The URL in §2c must **exactly** match `DISCORD_REDIRECT_URI` (scheme, host, path, no trailing slash). |
| `invalid_client` | Wrong/rotated `DISCORD_CLIENT_SECRET`; reset it in the portal and re-set the Fly secret. |
| Sign-in works locally but not on Fly | Set `DISCORD_REDIRECT_URI` explicitly - Fly's proxy host differs from the public URL. |
| Admin menu missing | Your `discord_id` doesn't match `ADMIN_DISCORD_ID` (check for stray spaces), or you're not signed in. |
| Stats reset after deploy | SQLite is writing to the container, not the Volume - make sure the Fly Volume is mounted at the path the server uses (`/data`) in `fly.toml`. |
| Bot can't DM a player | The player hasn't linked Discord / opted in, or shares no mutual context - fall back to the channel webhook. |
| Nothing happens on login | `DISCORD_CLIENT_ID`/`SECRET` not set; check server logs at startup for which features are enabled. |

---

## Code map (for maintainers)

This maps onto the planned services layer (see
[`docs/architecture/`](architecture/) and `ARCHITECTURE.md`):

| Location | Responsibility |
|---|---|
| `apps/server/src/auth_discord.rs` | OAuth2 code exchange, `/auth/discord/callback`, opaque session issue/verify |
| `apps/server/src/discord_notify.rs` | Bot DMs + channel webhook senders (inert until token/URL set) |
| `apps/server/src/lobby.rs` | Rooms, 6-char codes, browse/join |
| `apps/server/src/admin.rs` | Admin API, gated by `ADMIN_DISCORD_ID` |
| `apps/server/src/db.rs` | SQLite schema: `users`, `sessions`, `matches`, `stat_samples` |
| `crates/stats` | Shared stat definitions sampled by the server's headless sim |
| `web/admin/` | Admin dashboard UI + charts |
| `.env.example` | The full variable list above, with placeholders |
