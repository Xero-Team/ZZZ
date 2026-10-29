use crate::{AsyncBody, HttpClient, HttpRequestExt};
use anyhow::{Context as _, Result, anyhow, bail};
use futures::AsyncReadExt;
use http::Request;
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use std::{sync::Arc, time::Duration};
use url::Url;

const GITHUB_API_URL: &str = "https://api.github.com";
const GITHUB_RELEASE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct GitHubLspBinaryVersion {
    pub name: String,
    pub url: String,
    pub digest: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct GithubRelease {
    pub tag_name: String,
    #[serde(rename = "prerelease")]
    pub pre_release: bool,
    pub assets: Vec<GithubReleaseAsset>,
    pub tarball_url: String,
    pub zipball_url: String,
}

#[derive(Deserialize, Debug)]
pub struct GithubReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default, deserialize_with = "deserialize_sha256_digest")]
    pub digest: Option<String>,
}

fn deserialize_sha256_digest<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    const PREFIX: &str = "sha256:";

    let mut digest = Option::<String>::deserialize(deserializer)?;
    if let Some(digest) = digest.as_mut()
        && digest.starts_with(PREFIX)
    {
        digest.replace_range(..PREFIX.len(), "");
    }
    Ok(digest)
}

/// Performs a GitHub API request and deserializes its JSON body.
///
/// `what` names the payload in the error messages, e.g. `"latest releases"`.
async fn github_api_get<T: DeserializeOwned>(
    url: &str,
    http: Arc<dyn HttpClient>,
    what: &str,
) -> anyhow::Result<T> {
    let request = github_api_request(url)?;

    let mut response = http
        .send(request)
        .await
        .with_context(|| format!("error fetching {what}"))?;
    let status = response.status();

    let mut body = Vec::new();
    response
        .body_mut()
        .read_to_end(&mut body)
        .await
        .with_context(|| format!("error reading {what}"))?;

    // GitHub answers failed requests with a JSON error body, which would be
    // reported as a deserialization error instead of the actual status.
    if !status.is_success() {
        let text = String::from_utf8_lossy(body.as_slice());
        bail!("status error {}, response: {text:?}", status.as_u16());
    }

    let parsed = serde_json::from_slice::<T>(body.as_slice()).map_err(|err| {
        log::error!("Error deserializing {what}: {err:?}");
        log::error!(
            "GitHub API response text: {:?}",
            String::from_utf8_lossy(body.as_slice())
        );
        anyhow!("error deserializing {what}: {err:?}")
    })?;
    Ok(parsed)
}

pub async fn latest_github_release(
    repo_name_with_owner: &str,
    require_assets: bool,
    pre_release: bool,
    http: Arc<dyn HttpClient>,
) -> anyhow::Result<GithubRelease> {
    let url = format!("{GITHUB_API_URL}/repos/{repo_name_with_owner}/releases");
    let releases = github_api_get::<Vec<GithubRelease>>(&url, http, "latest releases").await?;

    releases
        .into_iter()
        .filter(|release| !require_assets || !release.assets.is_empty())
        .find(|release| release.pre_release == pre_release)
        .with_context(|| {
            if pre_release {
                "finding a prerelease"
            } else {
                "finding a release"
            }
        })
}

fn github_api_request(url: &str) -> Result<Request<AsyncBody>> {
    Request::get(url)
        .follow_redirects(crate::RedirectPolicy::FollowAll)
        .timeout(GITHUB_RELEASE_REQUEST_TIMEOUT)
        .when_some(std::env::var("GITHUB_TOKEN").ok(), |builder, token| {
            builder.header("Authorization", format!("Bearer {}", token))
        })
        .body(Default::default())
        .map_err(Into::into)
}

pub async fn get_release_by_tag_name(
    repo_name_with_owner: &str,
    tag: &str,
    http: Arc<dyn HttpClient>,
) -> anyhow::Result<GithubRelease> {
    let url = format!("{GITHUB_API_URL}/repos/{repo_name_with_owner}/releases/tags/{tag}");
    github_api_get(&url, http, "GitHub release").await
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AssetKind {
    TarGz,
    TarBz2,
    Gz,
    Zip,
}

pub fn build_asset_url(repo_name_with_owner: &str, tag: &str, kind: AssetKind) -> Result<String> {
    let mut url = Url::parse(&format!(
        "https://github.com/{repo_name_with_owner}/archive/refs/tags",
    ))?;
    // We're pushing this here, because tags may contain `/` and other characters
    // that need to be escaped.
    let asset_filename = format!(
        "{tag}.{extension}",
        extension = match kind {
            AssetKind::TarGz => "tar.gz",
            AssetKind::TarBz2 => "tar.bz2",
            AssetKind::Gz => "gz",
            AssetKind::Zip => "zip",
        }
    );
    url.path_segments_mut()
        .map_err(|()| anyhow!("cannot modify url path segments"))?
        .push(&asset_filename);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    /// Returns a fixed status and body for every request.
    struct StaticResponseClient {
        status: StatusCode,
        body: &'static str,
    }

    impl crate::HttpClient for StaticResponseClient {
        fn send(
            &self,
            _req: http::Request<crate::AsyncBody>,
        ) -> futures::future::BoxFuture<'static, anyhow::Result<http::Response<crate::AsyncBody>>>
        {
            let status = self.status;
            let body = self.body;
            Box::pin(async move {
                Ok(http::Response::builder()
                    .status(status)
                    .body(crate::AsyncBody::from(body))?)
            })
        }

        fn user_agent(&self) -> Option<&http::HeaderValue> {
            None
        }

        fn proxy(&self) -> Option<&url::Url> {
            None
        }
    }

    #[test]
    fn failed_requests_report_the_status_instead_of_a_deserialization_error() {
        // GitHub sends a JSON error body with failed requests, which used to be
        // reported as a malformed release list.
        let client = Arc::new(StaticResponseClient {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: r#"{"message":"Server Error"}"#,
        });

        let error = futures::executor::block_on(latest_github_release(
            "zed-industries/zed",
            false,
            false,
            client,
        ))
        .unwrap_err();

        assert!(error.to_string().contains("status error 500"), "{error}");
        assert!(!error.to_string().contains("deserializing"), "{error}");
    }

    #[test]
    fn successful_requests_are_deserialized() {
        let client = Arc::new(StaticResponseClient {
            status: StatusCode::OK,
            body: r#"[{"tag_name":"v1.0.0","prerelease":false,"assets":[],"tarball_url":"t","zipball_url":"z"}]"#,
        });

        let release = futures::executor::block_on(latest_github_release(
            "zed-industries/zed",
            false,
            false,
            client,
        ))
        .unwrap();

        assert_eq!(release.tag_name, "v1.0.0");
    }

    #[test]
    fn test_asset_digest_deserialization() {
        let parse = |json: &str| {
            serde_json::from_str::<GithubReleaseAsset>(json)
                .unwrap()
                .digest
        };

        assert_eq!(
            parse(r#"{"name":"a","browser_download_url":"u","digest":"sha256:abc"}"#),
            Some("abc".to_string())
        );
        assert_eq!(
            parse(r#"{"name":"a","browser_download_url":"u","digest":"abc"}"#),
            Some("abc".to_string())
        );
        assert_eq!(
            parse(r#"{"name":"a","browser_download_url":"u","digest":null}"#),
            None
        );
        assert_eq!(parse(r#"{"name":"a","browser_download_url":"u"}"#), None);
    }

    #[test]
    fn test_build_asset_url() {
        let tag = "release/2.3.5";
        let repo_name_with_owner = "microsoft/vscode-eslint";

        let tarball = build_asset_url(repo_name_with_owner, tag, AssetKind::TarGz).unwrap();
        assert_eq!(
            tarball,
            "https://github.com/microsoft/vscode-eslint/archive/refs/tags/release%2F2.3.5.tar.gz"
        );

        let zip = build_asset_url(repo_name_with_owner, tag, AssetKind::Zip).unwrap();
        assert_eq!(
            zip,
            "https://github.com/microsoft/vscode-eslint/archive/refs/tags/release%2F2.3.5.zip"
        );
    }
}
