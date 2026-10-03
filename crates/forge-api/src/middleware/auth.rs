use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use forge_auth::{Claims, JwtService};
use std::sync::Arc;

pub async fn require_auth(
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req.headers().get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .filter(|s| s.starts_with("Bearer "))
        .map(|s| s[7..].to_string());

    let token = match auth_header {
        Some(token) => token,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // In a real application, the JwtService would be passed via axum state,
    // but for this implementation we'll instantiate it with the known secret.
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "super_secret_key_for_testing".to_string());
    let jwt_service = JwtService::new(&secret);

    match jwt_service.validate_token(&token) {
        Ok(claims) => {
            // Store the authenticated claims in the request extensions so route handlers can access it
            req.extensions_mut().insert(Arc::new(claims));
            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}
