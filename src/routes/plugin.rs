use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::error::AppError;
use crate::models::condition::Condition;
use crate::schema;
use crate::services::sync::ConfigSyncEvent;
use crate::AppState;

fn extract_token(headers: &HeaderMap) -> Result<String, AppError> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = auth.strip_prefix("Token ").ok_or(AppError::Unauthorized)?;
    Ok(token.to_string())
}

#[derive(Deserialize)]
pub struct RegisterBody {
    pub guild_id: String,
    pub role_id: String,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> Result<Json<Value>, AppError> {
    let token = extract_token(&headers)?;

    sqlx::query(
        "INSERT INTO role_links (guild_id, role_id, api_token) VALUES ($1, $2, $3) \
         ON CONFLICT (guild_id, role_id) DO UPDATE SET api_token = $3, updated_at = now()",
    )
    .bind(&body.guild_id)
    .bind(&body.role_id)
    .bind(&token)
    .execute(&state.pool)
    .await?;

    tracing::info!(
        guild_id = body.guild_id,
        role_id = body.role_id,
        "Role link registered"
    );

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let token = extract_token(&headers)?;

    let link = sqlx::query_as::<_, (String, sqlx::types::Json<Vec<Condition>>)>(
        "SELECT guild_id, conditions FROM role_links WHERE api_token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    let verify_url = format!("{}/verify", state.config.base_url);
    let schema = schema::build_config_schema(&link.1, &verify_url);

    Ok(Json(schema))
}

#[derive(Deserialize)]
pub struct ConfigBody {
    pub guild_id: String,
    pub role_id: String,
    pub config: HashMap<String, Value>,
}

pub async fn post_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ConfigBody>,
) -> Result<Json<Value>, AppError> {
    let token = extract_token(&headers)?;

    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM role_links WHERE guild_id = $1 AND role_id = $2 AND api_token = $3)",
    )
    .bind(&body.guild_id)
    .bind(&body.role_id)
    .bind(&token)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if !exists {
        return Err(AppError::Unauthorized);
    }

    let conditions = schema::parse_config(&body.config)?;

    sqlx::query(
        "UPDATE role_links SET conditions = $1, updated_at = now() WHERE guild_id = $2 AND role_id = $3",
    )
    .bind(sqlx::types::Json(&conditions))
    .bind(&body.guild_id)
    .bind(&body.role_id)
    .execute(&state.pool)
    .await?;

    // Update tracked_subreddits eagerly
    update_tracked_subreddits(&state.pool).await;

    tracing::info!(
        guild_id = body.guild_id,
        role_id = body.role_id,
        count = conditions.len(),
        "Config updated"
    );

    // Trigger re-evaluation
    let _ = state
        .config_sync_tx
        .send(ConfigSyncEvent {
            guild_id: body.guild_id,
            role_id: body.role_id,
        })
        .await;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
pub struct DeleteConfigBody {
    pub guild_id: String,
    pub role_id: String,
}

pub async fn delete_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DeleteConfigBody>,
) -> Result<Json<Value>, AppError> {
    let token = extract_token(&headers)?;

    let result = sqlx::query(
        "DELETE FROM role_links WHERE guild_id = $1 AND role_id = $2 AND api_token = $3",
    )
    .bind(&body.guild_id)
    .bind(&body.role_id)
    .bind(&token)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Unauthorized);
    }

    // Update tracked_subreddits after deletion
    update_tracked_subreddits(&state.pool).await;

    tracing::info!(
        guild_id = body.guild_id,
        role_id = body.role_id,
        "Role link deleted"
    );

    Ok(Json(serde_json::json!({"success": true})))
}

/// Rebuild tracked_subreddits from all role_links conditions.
async fn update_tracked_subreddits(pool: &sqlx::PgPool) {
    let rows = sqlx::query_as::<_, (sqlx::types::Json<Vec<Condition>>,)>(
        "SELECT conditions FROM role_links WHERE conditions != '[]'",
    )
    .fetch_all(pool)
    .await;

    let Ok(rows) = rows else {
        return;
    };

    let mut counts: HashMap<String, i32> = HashMap::new();
    for (conditions,) in &rows {
        for c in conditions.iter() {
            if let Some(sub) = &c.subreddit {
                *counts.entry(sub.to_lowercase()).or_default() += 1;
            }
        }
    }

    let Ok(mut tx) = pool.begin().await else {
        return;
    };

    let _ = sqlx::query("DELETE FROM tracked_subreddits")
        .execute(&mut *tx)
        .await;

    for (sub, count) in &counts {
        let _ = sqlx::query(
            "INSERT INTO tracked_subreddits (subreddit, referenced_by, updated_at) VALUES ($1, $2, now())",
        )
        .bind(sub)
        .bind(count)
        .execute(&mut *tx)
        .await;
    }

    let _ = tx.commit().await;
}
