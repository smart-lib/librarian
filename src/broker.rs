use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{config::Config, db::Database, secrets};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProviderProxyPolicyRoute {
    pub provider: &'static str,
    pub method: &'static str,
    pub path: &'static str,
}

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
    ensure_proxy_policy(&provider, Method::POST, &path)?;
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

fn ensure_proxy_policy(provider: &str, method: Method, path: &str) -> Result<()> {
    let normalized = normalize_proxy_path(path)?;
    let method_label = method.as_str().to_string();
    if !proxy_policy_allows_normalized(provider, method, &normalized) {
        anyhow::bail!(
            "Provider proxy `{provider}` does not allow {} /{}",
            method_label,
            normalized
        );
    }
    Ok(())
}

fn normalize_proxy_path(path: &str) -> Result<String> {
    let normalized = path.trim().trim_start_matches('/').to_ascii_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("Provider proxy path must not be empty");
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        anyhow::bail!("Provider proxy path is not allowed");
    }
    Ok(normalized)
}

pub(crate) fn provider_proxy_policy_routes() -> Vec<ProviderProxyPolicyRoute> {
    vec![
        ProviderProxyPolicyRoute {
            provider: "openrouter",
            method: "POST",
            path: "api/v1/chat/completions",
        },
        ProviderProxyPolicyRoute {
            provider: "openai",
            method: "POST",
            path: "v1/chat/completions",
        },
        ProviderProxyPolicyRoute {
            provider: "openai",
            method: "POST",
            path: "v1/responses",
        },
        ProviderProxyPolicyRoute {
            provider: "openai",
            method: "POST",
            path: "v1/embeddings",
        },
    ]
}

pub(crate) fn provider_proxy_policy_allows(provider: &str, method: &str, path: &str) -> bool {
    let Ok(method) = method.parse::<Method>() else {
        return false;
    };
    let Ok(normalized) = normalize_proxy_path(path) else {
        return false;
    };
    proxy_policy_allows_normalized(provider, method, &normalized)
}

fn proxy_policy_allows_normalized(provider: &str, method: Method, path: &str) -> bool {
    if method != Method::POST {
        return false;
    }
    match provider {
        "openrouter" => matches!(path, "api/v1/chat/completions"),
        "openai" => matches!(
            path,
            "v1/chat/completions" | "v1/responses" | "v1/embeddings"
        ),
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_policy_allows_only_expected_provider_paths() {
        assert!(proxy_policy_allows_normalized(
            "openrouter",
            Method::POST,
            "api/v1/chat/completions"
        ));
        assert!(proxy_policy_allows_normalized(
            "openai",
            Method::POST,
            "v1/responses"
        ));
        assert!(proxy_policy_allows_normalized(
            "openai",
            Method::POST,
            "v1/embeddings"
        ));
        assert!(!proxy_policy_allows_normalized(
            "openrouter",
            Method::POST,
            "api/v1/credits"
        ));
        assert!(!proxy_policy_allows_normalized(
            "openrouter",
            Method::GET,
            "api/v1/chat/completions"
        ));
        assert!(!proxy_policy_allows_normalized(
            "unknown",
            Method::POST,
            "v1/chat/completions"
        ));
    }

    #[test]
    fn exported_proxy_policy_matches_runtime_checks() {
        let routes = provider_proxy_policy_routes();
        assert_eq!(routes.len(), 4);
        for route in routes {
            assert!(provider_proxy_policy_allows(
                route.provider,
                route.method,
                route.path
            ));
        }
        assert!(!provider_proxy_policy_allows(
            "openrouter",
            "GET",
            "api/v1/chat/completions"
        ));
        assert!(!provider_proxy_policy_allows(
            "openrouter",
            "POST",
            "api/v1/../secrets"
        ));
    }

    #[test]
    fn proxy_path_normalization_rejects_traversal_and_empty_segments() {
        assert_eq!(
            normalize_proxy_path("/API/v1/chat/completions").expect("path"),
            "api/v1/chat/completions"
        );
        assert!(normalize_proxy_path("").is_err());
        assert!(normalize_proxy_path("api//v1/chat/completions").is_err());
        assert!(normalize_proxy_path("api/v1/../secrets").is_err());
        assert!(ensure_proxy_policy("openrouter", Method::POST, "api/v1/credits").is_err());
    }
}
