use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubToken {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceFlowError {
    AuthorizationPending,
    SlowDown,
    ExpiredToken,
    AccessDenied,
    Other(String),
}

pub struct GitHubAuth;

impl GitHubAuth {
    pub async fn start_device_flow(client_id: &str) -> Result<DeviceCodeResponse> {
        let client = reqwest::Client::new();
        let resp = client
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("scope", "repo read:org"),
            ])
            .send()
            .await
            .context("failed to request device code from GitHub")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "GitHub device code request failed ({}): {}",
                status,
                body
            );
        }

        let dcr: DeviceCodeResponse = resp
            .json()
            .await
            .context("failed to parse device code response")?;
        Ok(dcr)
    }

    pub async fn poll_for_token(
        client_id: &str,
        device_code: &str,
        interval: u64,
    ) -> Result<Result<GitHubToken, DeviceFlowError>> {
        let client = reqwest::Client::new();

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

        let resp = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .context("failed to poll GitHub for token")?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: Option<String>,
            token_type: Option<String>,
            scope: Option<String>,
            error: Option<String>,
            error_description: Option<String>,
            interval: Option<u64>,
        }

        let tr: TokenResponse = resp
            .json()
            .await
            .context("failed to parse token response")?;

        if let Some(ref error) = tr.error {
            return match error.as_str() {
                "authorization_pending" => Ok(Err(DeviceFlowError::AuthorizationPending)),
                "slow_down" => Ok(Err(DeviceFlowError::SlowDown)),
                "expired_token" => Ok(Err(DeviceFlowError::ExpiredToken)),
                "access_denied" => Ok(Err(DeviceFlowError::AccessDenied)),
                other => Ok(Err(DeviceFlowError::Other(
                    tr.error_description
                        .clone()
                        .unwrap_or_else(|| other.to_string()),
                ))),
            };
        }

        let token = GitHubToken {
            access_token: tr
                .access_token
                .context("no access_token in response")?,
            token_type: tr.token_type.unwrap_or_else(|| "bearer".to_string()),
            scope: tr.scope.unwrap_or_default(),
        };

        Ok(Ok(token))
    }
}

// Standalone function re-exports for convenience
pub async fn start_device_flow(client_id: &str) -> Result<DeviceCodeResponse> {
    GitHubAuth::start_device_flow(client_id).await
}

pub async fn poll_for_token(
    client_id: &str,
    device_code: &str,
    interval: u64,
) -> Result<Result<GitHubToken, DeviceFlowError>> {
    GitHubAuth::poll_for_token(client_id, device_code, interval).await
}