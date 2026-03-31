-- Core role links (one per guild+role pair registered via POST /register)
CREATE TABLE IF NOT EXISTS role_links (
    id          BIGSERIAL PRIMARY KEY,
    guild_id    TEXT NOT NULL,
    role_id     TEXT NOT NULL,
    api_token   TEXT NOT NULL,
    conditions  JSONB NOT NULL DEFAULT '[]',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, role_id)
);

-- Account links: Discord <-> Reddit
CREATE TABLE IF NOT EXISTS linked_accounts (
    id              BIGSERIAL PRIMARY KEY,
    discord_id      TEXT NOT NULL UNIQUE,
    reddit_id       TEXT NOT NULL UNIQUE,
    reddit_username TEXT NOT NULL,
    linked_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Reddit OAuth refresh tokens (permanent, don't rotate)
CREATE TABLE IF NOT EXISTS reddit_tokens (
    reddit_id       TEXT PRIMARY KEY,
    refresh_token   TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- User data cache (denormalized for SQL filtering)
CREATE TABLE IF NOT EXISTS user_cache (
    reddit_id        TEXT PRIMARY KEY,
    user_data        JSONB NOT NULL DEFAULT '{}',
    total_karma      INTEGER NOT NULL DEFAULT 0,
    post_karma       INTEGER NOT NULL DEFAULT 0,
    comment_karma    INTEGER NOT NULL DEFAULT 0,
    account_age_days INTEGER NOT NULL DEFAULT 0,
    email_verified   BOOLEAN NOT NULL DEFAULT false,
    has_premium      BOOLEAN NOT NULL DEFAULT false,
    account_created  TIMESTAMPTZ,
    fetched_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    next_fetch_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    fetch_failures   INTEGER NOT NULL DEFAULT 0
);

-- Per-subreddit data (only for tracked subreddits)
CREATE TABLE IF NOT EXISTS user_subreddit_data (
    reddit_id      TEXT NOT NULL,
    subreddit      TEXT NOT NULL,
    is_subscriber  BOOLEAN,
    is_moderator   BOOLEAN,
    post_karma     INTEGER NOT NULL DEFAULT 0,
    comment_karma  INTEGER NOT NULL DEFAULT 0,
    post_count     INTEGER,
    comment_count  INTEGER,
    fetched_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (reddit_id, subreddit)
);

-- Tracks which subreddits are referenced in conditions
CREATE TABLE IF NOT EXISTS tracked_subreddits (
    subreddit     TEXT PRIMARY KEY,
    referenced_by INTEGER NOT NULL DEFAULT 1,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Role assignments (local mirror)
CREATE TABLE IF NOT EXISTS role_assignments (
    guild_id    TEXT NOT NULL,
    role_id     TEXT NOT NULL,
    discord_id  TEXT NOT NULL,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, role_id, discord_id),
    FOREIGN KEY (guild_id, role_id) REFERENCES role_links (guild_id, role_id) ON DELETE CASCADE
);

-- OAuth state (CSRF protection)
CREATE TABLE IF NOT EXISTS oauth_states (
    state        TEXT PRIMARY KEY,
    redirect_data JSONB,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Discord guild memberships
CREATE TABLE IF NOT EXISTS user_guilds (
    discord_id TEXT NOT NULL,
    guild_id   TEXT NOT NULL,
    guild_name TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (discord_id, guild_id)
);

-- Discord OAuth refresh tokens
CREATE TABLE IF NOT EXISTS discord_tokens (
    discord_id          TEXT PRIMARY KEY,
    refresh_token       TEXT NOT NULL,
    guilds_refreshed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
