# Reddit Member Role

A [RoleLogic](https://rolelogic.faizo.net) plugin that assigns Discord roles based on Reddit account stats. Discord admins configure conditions like "at least 1,000 karma" or "subscribed to r/rust", and verified members automatically receive (or lose) the role as their Reddit data changes.

> **Requires [Auth Gateway](../Auth-Gateway/)** — Discord login is handled by the centralized Auth Gateway. This plugin reads the shared `rl_session` cookie set by the gateway. Reddit OAuth for account linking is handled directly by this plugin.

## How It Works

1. **Users verify** — sign in with Discord (via Auth Gateway), then connect their Reddit account at the verification page.
2. **Admins configure** — In the RoleLogic dashboard, set conditions on any combination of Reddit fields (karma, account age, subreddit membership, etc.).
3. **Plugin syncs** — A background worker periodically refreshes Reddit data and evaluates conditions, adding or removing Discord roles via the RoleLogic API.

## Condition Fields

| Field | Type | Notes |
|-------|------|-------|
| Total Karma | numeric | All-time combined karma |
| Post Karma | numeric | Posts only |
| Comment Karma | numeric | Comments only |
| Subreddit Karma | numeric | Karma in a specific subreddit |
| Account Age (days) | numeric | Days since account creation |
| Email Verified | boolean | Reddit email verified |
| Reddit Premium | boolean | Active Reddit Premium/Gold |
| Subreddit Subscriber | boolean | Subscribed to a subreddit |
| Subreddit Moderator | boolean | Moderator of a subreddit |
| Posts in Subreddit | numeric | Post count in a subreddit (max 1000) |
| Comments in Subreddit | numeric | Comment count in a subreddit (max 1000) |

Numeric fields support operators: `=`, `>`, `>=`, `<`, `<=`, `between`.

## Setup

### Prerequisites

- Docker & Docker Compose
- [Auth Gateway](../Auth-Gateway/) running on `your-domain.com/auth/*`
- A [Reddit application](https://www.reddit.com/prefs/apps) (web app type) with redirect URI: `https://your-domain.com/reddit-member-role/verify/callback`

### Configuration

Copy `.env.example` to `.env` and fill in the values:

```env
# Database
DATABASE_URL=postgres://app:password@db:5432/reddit_member_role

# Reddit OAuth
REDDIT_CLIENT_ID=
REDDIT_CLIENT_SECRET=
REDDIT_USER_AGENT=RedditMemberRole/1.0

# Session signing (must match Auth Gateway)
SESSION_SECRET=

# Public URL (includes path prefix)
BASE_URL=https://your-domain.com/reddit-member-role

# Optional
LISTEN_ADDR=0.0.0.0:8080
RUST_LOG=reddit_member_role=info,tower_http=info
REDDIT_MAX_USERS_PER_HOUR=200
```

### Run

```bash
cp .env.example .env
# Fill in .env values
docker compose up -d
```

## Architecture

- **Rust + Axum 0.8 + PostgreSQL 16 + SQLx** — no ORM, no Redis, single binary
- **Background workers**: refresh (Reddit data), player sync (per-user role eval), config sync (bulk re-eval on config change)
- **Resource usage**: ~30-50 MB RAM, fits on a $4-6/month VPS alongside PostgreSQL

## Endpoints

All routes are nested under `/reddit-member-role`:

| Endpoint | Auth | Purpose |
|----------|------|---------|
| `POST /register` | Token | RoleLogic registers a role link |
| `GET /config` | Token | Returns config schema for dashboard |
| `POST /config` | Token | Saves admin conditions |
| `DELETE /config` | Token | Removes role link |
| `GET /verify` | -- | User verification page |
| `GET /verify/login` | -- | Redirects to Auth Gateway for Discord login |
| `GET /verify/reddit` | Session | Start Reddit OAuth flow |
| `GET /verify/callback` | -- | Reddit OAuth callback |
| `GET /health` | -- | Health check |

## License

[MIT](LICENSE) -- Copyright (c) 2026 faizo
