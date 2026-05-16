use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{config::Config, db::Database, secrets};

#[derive(Clone)]
struct BrokerState {
    db: Database,
    config: Config,
}

#[derive(Debug, Deserialize)]
struct ResolveRequest {
    token: String,
    capability: Option<String>,
}

pub async fn serve(bind: String, db: Database, config: Config) -> Result<()> {
    let state = BrokerState { db, config };
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/secrets/resolve", post(resolve_secret))
        .route("/v1/proxy/:provider/*path", post(proxy_provider))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Librarian broker listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn resolve_secret(
    State(state): State<BrokerState>,
    Json(input): Json<ResolveRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let grant_id = secrets::decode_grant_token(&input.token)?;
    let vault = secrets::SecretVault::new(state.config);
    let resolved = vault
        .resolve_with_grant(
            &state.db,
            grant_id,
            input.capability.as_deref().unwrap_or("read"),
            None,
        )
        .await?;
    Ok(Json(serde_json::json!({
        "name": resolved.name,
        "provider": resolved.provider,
        "kind": resolved.kind,
        "value": resolved.plaintext,
    })))
}

async fn proxy_provider(
    State(state): State<BrokerState>,
    AxumPath((provider, path)): AxumPath<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let token = headers
        .get("x-librarian-grant-token")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("Missing x-librarian-grant-token header"))?;
    let grant_id = secrets::decode_grant_token(token)?;
    let vault = secrets::SecretVault::new(state.config);
    let resolved = vault
        .resolve_with_grant(&state.db, grant_id, "provider-proxy", None)
        .await?;
    if resolved.provider != provider {
        return Err(anyhow::anyhow!(
            "Grant provider mismatch: grant is for {}, request is for {}",
            resolved.provider,
            provider
        )
        .into());
    }

    let base = provider_base_url(&provider)?;
    let url = format!("{}/{}", base.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .bearer_auth(resolved.plaintext)
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await?;
    let status = StatusCode::from_u16(response.status().as_u16())?;
    let bytes = response.bytes().await?;
    Ok((status, bytes))
}

fn provider_base_url(provider: &str) -> Result<&'static str> {
    match provider {
        "openai" => Ok("https://api.openai.com"),
        "openrouter" => Ok("https://openrouter.ai"),
        _ => anyhow::bail!("Provider proxy `{provider}` is not configured"),
    }
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}
