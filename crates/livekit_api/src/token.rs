use anyhow::Result;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    ops::Add,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DEFAULT_TTL: Duration = Duration::from_secs(6 * 60 * 60); // 6 hours

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimGrants<'a> {
    pub iss: Cow<'a, str>,
    pub sub: Option<Cow<'a, str>>,
    pub iat: u64,
    pub exp: u64,
    pub nbf: u64,
    pub jwtid: Option<Cow<'a, str>>,
    pub video: VideoGrant<'a>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoGrant<'a> {
    pub room_create: Option<bool>,
    pub room_join: Option<bool>,
    pub room_list: Option<bool>,
    pub room_record: Option<bool>,
    pub room_admin: Option<bool>,
    pub room: Option<Cow<'a, str>>,
    pub can_publish: Option<bool>,
    pub can_subscribe: Option<bool>,
    pub can_publish_data: Option<bool>,
    pub hidden: Option<bool>,
    pub recorder: Option<bool>,
}

impl<'a> VideoGrant<'a> {
    pub fn to_admin(room: &'a str) -> Self {
        Self {
            room_admin: Some(true),
            room: Some(Cow::Borrowed(room)),
            ..Default::default()
        }
    }

    pub fn to_join(room: &'a str) -> Self {
        Self {
            room: Some(Cow::Borrowed(room)),
            room_join: Some(true),
            can_publish: Some(true),
            can_subscribe: Some(true),
            ..Default::default()
        }
    }

    pub fn for_guest(room: &'a str) -> Self {
        Self {
            room: Some(Cow::Borrowed(room)),
            room_join: Some(true),
            can_publish: Some(false),
            can_subscribe: Some(true),
            ..Default::default()
        }
    }
}

pub fn create(
    api_key: &str,
    secret_key: &str,
    identity: Option<&str>,
    video_grant: VideoGrant,
) -> Result<String> {
    if video_grant.room_join.is_some() && identity.is_none() {
        anyhow::bail!("identity is required for room_join grant, but it is none");
    }

    let now = SystemTime::now();

    let claims = ClaimGrants {
        iss: Cow::Borrowed(api_key),
        sub: identity.map(Cow::Borrowed),
        iat: now.duration_since(UNIX_EPOCH).unwrap().as_secs(),
        exp: now
            .add(DEFAULT_TTL)
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        nbf: 0,
        jwtid: identity.map(Cow::Borrowed),
        video: video_grant,
    };
    Ok(jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret_key.as_ref()),
    )?)
}

pub fn validate<'a>(token: &'a str, secret_key: &str) -> Result<ClaimGrants<'a>> {
    let token = jsonwebtoken::decode(
        token,
        &DecodingKey::from_secret(secret_key.as_ref()),
        &Validation::default(),
    )?;

    Ok(token.claims)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TTL, VideoGrant, create, validate};

    #[test]
    fn video_grant_helpers_set_expected_flags() {
        let admin = VideoGrant::to_admin("room-a");
        assert_eq!(admin.room_admin, Some(true));
        assert_eq!(admin.room.as_deref(), Some("room-a"));
        assert_eq!(admin.room_join, None);

        let join = VideoGrant::to_join("room-b");
        assert_eq!(join.room.as_deref(), Some("room-b"));
        assert_eq!(join.room_join, Some(true));
        assert_eq!(join.can_publish, Some(true));
        assert_eq!(join.can_subscribe, Some(true));

        let guest = VideoGrant::for_guest("room-c");
        assert_eq!(guest.room.as_deref(), Some("room-c"));
        assert_eq!(guest.room_join, Some(true));
        assert_eq!(guest.can_publish, Some(false));
        assert_eq!(guest.can_subscribe, Some(true));
    }

    #[test]
    fn create_requires_identity_for_join_grant() {
        let error = create("api-key", "secret", None, VideoGrant::to_join("room-a")).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("identity is required for room_join grant")
        );
    }

    #[test]
    fn create_and_validate_round_trip_claims() {
        let token = create(
            "api-key",
            "secret",
            Some("alice"),
            VideoGrant::for_guest("room-a"),
        )
        .unwrap();

        let claims = validate(&token, "secret").unwrap();
        assert_eq!(claims.iss.as_ref(), "api-key");
        assert_eq!(claims.sub.as_deref(), Some("alice"));
        assert_eq!(claims.jwtid.as_deref(), Some("alice"));
        assert_eq!(claims.video.room.as_deref(), Some("room-a"));
        assert_eq!(claims.video.room_join, Some(true));
        assert_eq!(claims.video.can_publish, Some(false));
        assert_eq!(claims.video.can_subscribe, Some(true));
        assert_eq!(claims.nbf, 0);
        assert!(claims.exp >= claims.iat + DEFAULT_TTL.as_secs());
    }

    #[test]
    fn validate_rejects_wrong_secret() {
        let token = create(
            "api-key",
            "secret",
            Some("alice"),
            VideoGrant::to_admin("room-a"),
        )
        .unwrap();

        assert!(validate(&token, "wrong-secret").is_err());
    }
}
