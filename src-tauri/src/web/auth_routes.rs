//! `/api/auth/*`: sign-up, sign-in, sign-out, recovery, sessions. All logic
//! lives in `auth::service`; these handlers only translate HTTP.

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::util::*;
use super::WebState;
use crate::auth::{service, SHORT_SESSION_HOURS, TRUSTED_SESSION_DAYS};
use crate::models::user::{DbUser, SessionInfo, SignupMode, UserInfo};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/status", get(status))
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/reset", post(reset))
        .route("/me", get(me))
        .route("/change-passphrase", post(change_passphrase))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", delete(revoke_session))
}

#[derive(Serialize)]
struct StatusBody {
    has_users: bool,
    signup_mode: SignupMode,
    user: Option<UserInfo>,
}

/// Public: what the sign-in page needs before anyone has typed anything.
async fn status(
    State(st): State<WebState>,
    headers: HeaderMap,
) -> Result<Json<StatusBody>, ApiError> {
    let user = match token_from_headers(&headers) {
        Some(t) => service::authenticate(&st.db, &t)
            .await?
            .map(|(u, _)| UserInfo::from(&u)),
        None => None,
    };
    Ok(Json(StatusBody {
        has_users: service::has_users(&st.db).await?,
        signup_mode: service::signup_mode(&st.db).await?,
        user,
    }))
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    passphrase: String,
    #[serde(default)]
    trusted: bool,
}

#[derive(Serialize)]
struct SignedIn {
    user: UserInfo,
    /// Shown once, when a new key was just issued (sign-up, recovery).
    recovery_key: Option<String>,
}

fn signed_in(user: &DbUser, token: &str, trusted: bool, recovery_key: Option<String>) -> Response {
    let max_age = if trusted {
        TRUSTED_SESSION_DAYS * 86_400
    } else {
        SHORT_SESSION_HOURS * 3_600
    };
    (
        [(header::SET_COOKIE, session_cookie(token, max_age))],
        Json(SignedIn {
            user: UserInfo::from(user),
            recovery_key,
        }),
    )
        .into_response()
}

async fn register(
    State(st): State<WebState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> Result<Response, ApiError> {
    if !st.limiter.allow(ip) {
        return Err(too_many());
    }
    let (user, key) = service::register(&st.db, &body.username, &body.passphrase).await?;
    let token =
        service::issue_session(&st.db, &user.id, &device_label(&headers), body.trusted).await?;
    Ok(signed_in(&user, &token, body.trusted, Some(key)))
}

async fn login(
    State(st): State<WebState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> Result<Response, ApiError> {
    if !st.limiter.allow(ip) {
        return Err(too_many());
    }
    let (user, token) = service::login(
        &st.db,
        &body.username,
        &body.passphrase,
        body.trusted,
        &device_label(&headers),
    )
    .await?;
    Ok(signed_in(&user, &token, body.trusted, None))
}

async fn logout(State(st): State<WebState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if let Some(token) = token_from_headers(&headers) {
        service::logout(&st.db, &token).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie())],
    )
        .into_response())
}

#[derive(Deserialize)]
struct ResetBody {
    username: String,
    recovery_key: String,
    new_passphrase: String,
    #[serde(default)]
    trusted: bool,
}

async fn reset(
    State(st): State<WebState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<ResetBody>,
) -> Result<Response, ApiError> {
    if !st.limiter.allow(ip) {
        return Err(too_many());
    }
    let (user, new_key) = service::reset_with_recovery(
        &st.db,
        &body.username,
        &body.recovery_key,
        &body.new_passphrase,
    )
    .await?;
    let token =
        service::issue_session(&st.db, &user.id, &device_label(&headers), body.trusted).await?;
    Ok(signed_in(&user, &token, body.trusted, Some(new_key)))
}

async fn me(authed: AuthedUser) -> Json<UserInfo> {
    Json(UserInfo::from(&authed.user))
}

#[derive(Deserialize)]
struct ChangePassphrase {
    current: String,
    new: String,
}

async fn change_passphrase(
    State(st): State<WebState>,
    authed: AuthedUser,
    Json(body): Json<ChangePassphrase>,
) -> Result<StatusCode, ApiError> {
    service::change_passphrase(
        &st.db,
        &authed.user.id,
        &body.current,
        &body.new,
        Some(&authed.session.id),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_sessions(
    State(st): State<WebState>,
    ActiveUser(authed): ActiveUser,
) -> Result<Json<Vec<SessionInfo>>, ApiError> {
    Ok(Json(
        service::list_sessions(&st.db, &authed.user.id, Some(&authed.session.id)).await?,
    ))
}

async fn revoke_session(
    State(st): State<WebState>,
    ActiveUser(authed): ActiveUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    service::revoke_session(&st.db, &authed.user.id, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
