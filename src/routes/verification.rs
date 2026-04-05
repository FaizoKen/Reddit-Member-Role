use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar};
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::services::reddit_oauth::RedditOAuth;
use crate::services::session;
use crate::services::sync::PlayerSyncEvent;
use crate::AppState;

const SESSION_COOKIE: &str = "rl_session";

fn get_session(jar: &CookieJar, secret: &str) -> Result<(String, String), AppError> {
    let cookie = jar.get(SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
    session::verify_session(cookie.value(), secret).ok_or(AppError::Unauthorized)
}

pub fn render_verify_page(base_url: &str) -> String {
    let login_url = format!("{base_url}/verify/login");

    format!(
        r##"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Reddit Member Role - Link Account</title>
    <link rel="icon" href="{base_url}/favicon.ico" type="image/x-icon">
    <meta name="description" content="Link your Discord and Reddit accounts to automatically receive server roles based on your Reddit membership and stats.">
    <meta name="theme-color" content="#ff4500">
    <style>
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{ font-family: system-ui, -apple-system, sans-serif; max-width: 580px; margin: 0 auto; padding: 32px 20px; background: #0e1525; color: #c8ccd4; min-height: 100vh; }}
        h1 {{ color: #ff4500; font-size: 24px; margin-bottom: 4px; }}
        h2 {{ color: #fff; font-size: 17px; margin-bottom: 14px; }}
        p {{ line-height: 1.6; margin: 6px 0; font-size: 14px; }}
        a {{ color: #74b9ff; }}
        .subtitle {{ color: #7a8299; font-size: 14px; margin-bottom: 20px; }}
        .card {{ background: #161d2e; padding: 22px; border-radius: 10px; margin: 14px 0; border: 1px solid #1e2a3d; }}
        .btn {{ display: inline-flex; align-items: center; gap: 8px; padding: 10px 22px; color: #fff; text-decoration: none; border-radius: 6px; font-size: 14px; font-weight: 500; border: none; cursor: pointer; font-family: inherit; transition: background .15s; }}
        .btn-discord {{ background: #5865f2; }}
        .btn-discord:hover {{ background: #4752c4; }}
        .btn-reddit {{ background: #ff4500; }}
        .btn-reddit:hover {{ background: #cc3700; }}
        .btn-danger {{ background: transparent; color: #f87171; border: 1px solid #7f1d1d; font-size: 13px; padding: 8px 16px; }}
        .btn-danger:hover {{ background: #7f1d1d33; }}
        .btn:disabled {{ opacity: 0.5; cursor: not-allowed; }}
        .badge {{ display: inline-block; padding: 3px 10px; border-radius: 20px; font-size: 12px; font-weight: 500; }}
        .badge-ok {{ background: #052e16; color: #4ade80; border: 1px solid #14532d; }}
        .msg {{ padding: 10px 14px; border-radius: 6px; margin: 12px 0; font-size: 13px; line-height: 1.5; }}
        .msg-error {{ background: #1c0a0a; color: #fca5a5; border: 1px solid #7f1d1d; }}
        .msg-success {{ background: #052e16; color: #86efac; border: 1px solid #14532d; }}
        .info-row {{ display: flex; align-items: center; gap: 8px; margin: 6px 0; font-size: 14px; }}
        .info-row .label {{ color: #64748b; min-width: 80px; }}
        .info-row .val {{ color: #ff4500; font-weight: 600; }}
        .trust-note {{ font-size: 13px; color: #94a3b8; background: #111827; border-left: 3px solid #3b82f6; padding: 10px 14px; border-radius: 0 6px 6px 0; margin: 10px 0; line-height: 1.6; }}
        .trust-note strong {{ color: #e2e8f0; }}
        .btn-logout {{ background: transparent; color: #94a3b8; border: 1px solid #1e293b; padding: 5px 12px; border-radius: 6px; font-size: 12px; cursor: pointer; font-family: inherit; }}
        .btn-logout:hover {{ color: #f87171; border-color: #7f1d1d; }}
        .actions {{ display: flex; gap: 8px; margin-top: 16px; flex-wrap: wrap; }}
        .hidden {{ display: none !important; }}
        .divider {{ border: none; border-top: 1px solid #1e293b; margin: 16px 0; }}
    </style>
</head>
<body>
    <div style="display:flex; align-items:center; justify-content:space-between; margin-bottom:4px;">
        <div style="display:flex; align-items:center; gap:10px;">
            <h1 style="margin:0;">Reddit Member Role</h1>
            <span style="font-size:11px; color:#64748b; background:#1e293b; padding:2px 8px; border-radius:4px;">Powered by <a href="https://rolelogic.faizo.net" target="_blank" rel="noopener" style="color:#74b9ff; text-decoration:none;">RoleLogic</a></span>
        </div>
        <button id="logout-btn" class="btn-logout hidden" onclick="doLogout()">Logout</button>
    </div>
    <p class="subtitle">Link your Discord and Reddit accounts to automatically receive server roles based on your Reddit membership and stats.</p>

    <div id="loading-section" class="card"><p style="color: #64748b;">Loading...</p></div>

    <div id="login-section" class="card hidden">
        <h2>Step 1: Sign in with Discord</h2>
        <p>Sign in so we know which Discord account to assign roles to.</p>
        <p class="trust-note">We request the <strong>identify</strong> and <strong>guilds</strong> scopes only.</p>
        <div class="actions">
            <a href="{login_url}" class="btn btn-discord">
                <svg width="20" height="15" viewBox="0 0 71 55" fill="white"><path d="M60.1 4.9A58.5 58.5 0 0045.4.2a.2.2 0 00-.2.1 40.8 40.8 0 00-1.8 3.7 54 54 0 00-16.2 0A37.3 37.3 0 0025.4.3a.2.2 0 00-.2-.1A58.4 58.4 0 0010.6 4.9a.2.2 0 00-.1.1C1.5 18 -.9 30.6.3 43a.2.2 0 00.1.2 58.7 58.7 0 0017.7 9 .2.2 0 00.3-.1 42 42 0 003.6-5.9.2.2 0 00-.1-.3 38.6 38.6 0 01-5.5-2.6.2.2 0 01 0-.4l1.1-.9a.2.2 0 01.2 0 41.9 41.9 0 0035.6 0 .2.2 0 01.2 0l1.1.9a.2.2 0 010 .3 36.3 36.3 0 01-5.5 2.7.2.2 0 00-.1.3 47.2 47.2 0 003.6 5.9.2.2 0 00.3.1A58.5 58.5 0 0070.3 43a.2.2 0 00.1-.2c1.4-14.7-2.4-27.5-10.2-38.8a.2.2 0 00-.1 0zM23.7 35.3c-3.4 0-6.1-3.1-6.1-6.8s2.7-6.9 6.1-6.9 6.2 3.1 6.1 6.9c0 3.7-2.7 6.8-6.1 6.8zm22.6 0c-3.4 0-6.1-3.1-6.1-6.8s2.7-6.9 6.1-6.9 6.2 3.1 6.1 6.9c0 3.7-2.7 6.8-6.1 6.8z"/></svg>
                Login with Discord
            </a>
        </div>
    </div>

    <div id="reddit-section" class="card hidden">
        <h2>Step 2: Connect Reddit</h2>
        <p>Signed in as <span id="reddit-discord" style="color:#74b9ff;"></span></p>
        <p>Connect your Reddit account to complete the link.</p>
        <p class="trust-note">We request <strong>identity</strong>, <strong>read</strong>, <strong>mysubreddits</strong>, and <strong>history</strong> scopes. We only read your public profile data and cannot post on your behalf.</p>
        <div class="actions">
            <a id="reddit-link" href="#" class="btn btn-reddit">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="white"><path d="M12 0A12 12 0 000 12a12 12 0 0012 12 12 12 0 0012-12A12 12 0 0012 0zm5.01 13.38c.15.36.23.75.23 1.14 0 2.88-3.35 5.22-7.48 5.22s-7.48-2.34-7.48-5.22c0-.39.08-.78.23-1.14A1.56 1.56 0 012 12.08c0-.43.17-.83.49-1.14a1.59 1.59 0 012.2-.04c1.23-.87 2.92-1.43 4.78-1.49l.9-4.27a.34.34 0 01.4-.27l3.02.64a1.1 1.1 0 012.11.37 1.1 1.1 0 01-1.1 1.1 1.1 1.1 0 01-1.07-.87l-2.68-.57-.8 3.81c1.83.07 3.49.63 4.7 1.49a1.59 1.59 0 012.19.04c.31.31.49.71.49 1.14 0 .63-.37 1.16-.89 1.42zM8.07 12.07a1.34 1.34 0 00-1.34 1.34 1.34 1.34 0 001.34 1.34 1.34 1.34 0 001.34-1.34 1.34 1.34 0 00-1.34-1.34zm6.53 3.87a.24.24 0 00-.17.07 3.87 3.87 0 01-2.6.86 3.87 3.87 0 01-2.6-.86.24.24 0 00-.34.34 4.38 4.38 0 002.94.97 4.38 4.38 0 002.94-.97.24.24 0 00-.17-.41zm-.78-2.53a1.34 1.34 0 001.34 1.34 1.34 1.34 0 001.34-1.34 1.34 1.34 0 00-1.34-1.34 1.34 1.34 0 00-1.34 1.34z"/></svg>
                Connect Reddit
            </a>
        </div>
    </div>

    <div id="linked-section" class="card hidden">
        <div style="display:flex; align-items:center; gap:10px; margin-bottom:14px;">
            <h2 style="margin:0;">Accounts Linked</h2>
            <span class="badge badge-ok">Verified</span>
        </div>
        <div class="info-row"><span class="label">Reddit</span> <span class="val" id="linked-reddit"></span></div>
        <div class="info-row"><span class="label">Discord</span> <span class="val" id="linked-discord" style="color:#94a3b8;font-weight:400;font-size:13px;"></span></div>
        <p style="color:#4ade80; margin-top:12px; font-size:13px;">Your roles are assigned automatically based on your Reddit data.</p>
        <hr class="divider">
        <div class="actions">
            <button class="btn btn-danger" onclick="doUnlink()">Unlink Account</button>
        </div>
    </div>

    <div id="msg" class="hidden"></div>
    <noscript><p style="color:#f87171; margin-top:20px;">JavaScript is required.</p></noscript>

    <script>
    const API = '{base_url}';
    const REDDIT_URL = '{base_url}/verify/reddit';

    async function api(method, path, body) {{
        const opts = {{ method, headers: {{}}, credentials: 'include' }};
        if (body) {{
            opts.headers['Content-Type'] = 'application/json';
            opts.body = JSON.stringify(body);
        }}
        const res = await fetch(API + path, opts);
        const data = await res.json();
        if (!res.ok) throw new Error(data.error || 'Request failed');
        return data;
    }}

    function showSection(id) {{
        ['loading-section','login-section','reddit-section','linked-section'].forEach(s =>
            document.getElementById(s).classList.add('hidden')
        );
        document.getElementById(id).classList.remove('hidden');
    }}

    function showMsg(text, type) {{
        const el = document.getElementById('msg');
        el.className = 'msg msg-' + type;
        el.textContent = text;
        el.classList.remove('hidden');
        if (type === 'success') setTimeout(() => el.classList.add('hidden'), 6000);
    }}

    function clearMsg() {{ document.getElementById('msg').classList.add('hidden'); }}

    let currentName = '';

    async function init() {{
        try {{
            const s = await api('GET', '/verify/status');
            currentName = s.display_name || '';
            document.getElementById('logout-btn').classList.remove('hidden');
            if (s.linked) {{
                document.getElementById('linked-reddit').textContent = 'u/' + s.linked;
                document.getElementById('linked-discord').textContent = s.display_name;
                showSection('linked-section');
            }} else {{
                document.getElementById('reddit-discord').textContent = s.display_name;
                document.getElementById('reddit-link').href = REDDIT_URL;
                showSection('reddit-section');
            }}
        }} catch (e) {{
            showSection('login-section');
        }}
    }}

    async function doLogout() {{
        clearMsg();
        try {{
            await api('POST', '/verify/logout');
            document.getElementById('logout-btn').classList.add('hidden');
            showSection('login-section');
            showMsg('Logged out.', 'success');
        }} catch (e) {{ showMsg(e.message, 'error'); }}
    }}

    async function doUnlink() {{
        clearMsg();
        if (!confirm('Unlink your account? You will lose all assigned roles.')) return;
        try {{
            await api('POST', '/verify/unlink');
            document.getElementById('reddit-discord').textContent = currentName;
            document.getElementById('reddit-link').href = REDDIT_URL;
            showSection('reddit-section');
            showMsg('Account unlinked.', 'success');
        }} catch (e) {{ showMsg(e.message, 'error'); }}
    }}

    init();
    </script>
</body>
</html>"##
    )
}

pub async fn verify_page(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        state.verify_html.clone(),
    )
}

pub async fn login(State(state): State<Arc<AppState>>) -> Response {
    let return_to = "/reddit-member-role/verify";
    let url = format!(
        "/auth/login?return_to={}",
        urlencoding::encode(return_to),
    );
    Redirect::temporary(&url).into_response()
}

/// Redirect to Reddit OAuth (requires Discord session).
pub async fn reddit_login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Response, AppError> {
    let (discord_id, _) = get_session(&jar, &state.config.session_secret)?;

    let state_param: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let expires = chrono::Utc::now() + chrono::Duration::minutes(10);

    sqlx::query(
        "INSERT INTO oauth_states (state, redirect_data, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(&state_param)
    .bind(json!({"provider": "reddit", "discord_id": discord_id}))
    .bind(expires)
    .execute(&state.pool)
    .await?;

    let url = RedditOAuth::authorize_url(&state.config, &state_param);
    Ok(Redirect::temporary(&url).into_response())
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: String,
    pub error: Option<String>,
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Query(query): Query<CallbackQuery>,
) -> Result<(CookieJar, Redirect), AppError> {
    if query.error.is_some() || query.code.is_none() {
        return Ok((jar, Redirect::to(&format!("{}/verify", state.config.base_url))));
    }
    let code = query.code.unwrap();

    // Look up and validate state
    let state_row = sqlx::query_as::<_, (Value,)>(
        "SELECT redirect_data FROM oauth_states WHERE state = $1 AND expires_at > now()",
    )
    .bind(&query.state)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::BadRequest("Invalid or expired OAuth state".into()))?;

    sqlx::query("DELETE FROM oauth_states WHERE state = $1")
        .bind(&query.state)
        .execute(&state.pool)
        .await?;

    let redirect_data = state_row.0;
    let discord_id = redirect_data["discord_id"]
        .as_str()
        .ok_or(AppError::Internal("Missing discord_id in Reddit OAuth state".into()))?;

    handle_reddit_callback(&state, jar, &code, discord_id).await
}

async fn handle_reddit_callback(
    state: &Arc<AppState>,
    jar: CookieJar,
    code: &str,
    discord_id: &str,
) -> Result<(CookieJar, Redirect), AppError> {
    let reddit_oauth = RedditOAuth::with_client(state.http.clone());
    let (access_token, refresh_token) = reddit_oauth.exchange_code(&state.config, code).await?;

    // Fetch Reddit user info
    let user_data = state.reddit_client.fetch_user_data(&access_token).await
        .map_err(|e| AppError::Internal(format!("Failed to fetch Reddit user data: {e}")))?;

    let reddit_id = user_data.id.clone();
    let reddit_username = user_data.name.clone();

    // Check if this Reddit account is already linked to another Discord user
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT discord_id FROM linked_accounts WHERE reddit_id = $1",
    )
    .bind(&reddit_id)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(existing_discord) = existing {
        if existing_discord != discord_id {
            return Err(AppError::BadRequest(
                "This Reddit account is already linked to another Discord account".into(),
            ));
        }
    }

    // Check if this Discord user already has a linked Reddit account
    let existing_reddit = sqlx::query_scalar::<_, String>(
        "SELECT reddit_id FROM linked_accounts WHERE discord_id = $1",
    )
    .bind(discord_id)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(existing_rid) = existing_reddit {
        if existing_rid != reddit_id {
            return Err(AppError::BadRequest(
                "Your Discord account is already linked to a different Reddit account. Unlink it first.".into(),
            ));
        }
    }

    // Store refresh token
    if let Some(ref rt) = refresh_token {
        sqlx::query(
            "INSERT INTO reddit_tokens (reddit_id, refresh_token) VALUES ($1, $2) \
             ON CONFLICT (reddit_id) DO UPDATE SET refresh_token = $2, updated_at = now()",
        )
        .bind(&reddit_id)
        .bind(rt)
        .execute(&state.pool)
        .await?;
    }

    // Calculate account age
    let account_age_days = ((chrono::Utc::now().timestamp() as f64 - user_data.created_utc) / 86400.0) as i32;

    // Insert user cache
    let account_created = chrono::DateTime::from_timestamp(user_data.created_utc as i64, 0);
    sqlx::query(
        "INSERT INTO user_cache (reddit_id, user_data, total_karma, post_karma, comment_karma, \
         account_age_days, email_verified, has_premium, account_created) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT (reddit_id) DO UPDATE SET \
         user_data = $2, total_karma = $3, post_karma = $4, comment_karma = $5, \
         account_age_days = $6, email_verified = $7, has_premium = $8, \
         account_created = $9, fetched_at = now(), next_fetch_at = now(), fetch_failures = 0",
    )
    .bind(&reddit_id)
    .bind(json!({
        "name": user_data.name,
        "id": user_data.id,
        "total_karma": user_data.total_karma,
        "link_karma": user_data.link_karma,
        "comment_karma": user_data.comment_karma,
        "created_utc": user_data.created_utc,
        "has_verified_email": user_data.has_verified_email,
        "is_gold": user_data.is_gold,
    }))
    .bind(user_data.total_karma as i32)
    .bind(user_data.link_karma as i32)
    .bind(user_data.comment_karma as i32)
    .bind(account_age_days)
    .bind(user_data.has_verified_email)
    .bind(user_data.is_gold)
    .bind(account_created)
    .execute(&state.pool)
    .await?;

    // Fetch and store karma breakdown
    if let Ok(karma_list) = state.reddit_client.fetch_karma_breakdown(&access_token).await {
        for k in &karma_list {
            let sub = k.sr.to_lowercase();
            let _ = sqlx::query(
                "INSERT INTO user_subreddit_data (reddit_id, subreddit, post_karma, comment_karma) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (reddit_id, subreddit) DO UPDATE SET \
                 post_karma = $3, comment_karma = $4, fetched_at = now()",
            )
            .bind(&reddit_id)
            .bind(&sub)
            .bind(k.link_karma as i32)
            .bind(k.comment_karma as i32)
            .execute(&state.pool)
            .await;
        }
    }

    // Link accounts
    sqlx::query(
        "INSERT INTO linked_accounts (discord_id, reddit_id, reddit_username) VALUES ($1, $2, $3) \
         ON CONFLICT (discord_id) DO UPDATE SET reddit_id = $2, reddit_username = $3, linked_at = now()",
    )
    .bind(discord_id)
    .bind(&reddit_id)
    .bind(&reddit_username)
    .execute(&state.pool)
    .await?;

    // Trigger role sync
    let _ = state
        .player_sync_tx
        .send(PlayerSyncEvent::AccountLinked {
            discord_id: discord_id.to_string(),
        })
        .await;

    tracing::info!(discord_id, reddit_id, reddit_username, "Account linked");

    Ok((jar, Redirect::to(&format!("{}/verify", state.config.base_url))))
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Json<Value>, AppError> {
    let (discord_id, display_name) = get_session(&jar, &state.config.session_secret)?;

    let account = sqlx::query_as::<_, (String,)>(
        "SELECT reddit_username FROM linked_accounts WHERE discord_id = $1",
    )
    .bind(&discord_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(Json(json!({
        "discord_id": discord_id,
        "display_name": display_name,
        "linked": account.as_ref().map(|a| &a.0),
    })))
}

pub async fn logout(jar: CookieJar) -> (CookieJar, Json<Value>) {
    let cookie = Cookie::build(SESSION_COOKIE).path("/");
    let jar = jar.remove(cookie);
    (jar, Json(json!({"success": true})))
}

pub async fn unlink(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<Json<Value>, AppError> {
    let (discord_id, _) = get_session(&jar, &state.config.session_secret)?;

    let account = sqlx::query_as::<_, (String, String)>(
        "SELECT reddit_id, reddit_username FROM linked_accounts WHERE discord_id = $1",
    )
    .bind(&discord_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound("No linked account found".into()))?;

    // Delete linked account (keeps cache/tokens for cleanup later)
    sqlx::query("DELETE FROM linked_accounts WHERE discord_id = $1")
        .bind(&discord_id)
        .execute(&state.pool)
        .await?;

    // Trigger removal from all roles
    let _ = state
        .player_sync_tx
        .send(PlayerSyncEvent::AccountUnlinked {
            discord_id: discord_id.clone(),
        })
        .await;

    tracing::info!(discord_id, reddit_username = account.1, "Account unlinked");

    Ok(Json(json!({"success": true})))
}
