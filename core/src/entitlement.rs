//! Free and paid use. No server and no account: a purchase is proven on the device itself.
//!
//!  - Store purchases (Microsoft Store, App Store, Google Play): the platform shell asks the store's own API and hands
//!    the result in as a `StoreEntitlement` (`State::store`); the store, not this code, decides whether it is valid.
//!  - Stripe: the buyer receives a signed license key (`GIT1.<payload>.<signature>`, Ed25519) that `verify_license`
//!    checks against the public key built into the app. The private key stays with the operator
//!    (`cargo run -p git_core --example license_tool`).
//!
//! The free plan follows `FREE_MAX_FOLLOWS` sites and looks only after the first run of the day (a daily digest); the
//! paid plan has no such limits. Everything is a pure function of the state and the clock: easy to test.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Sites a free user can follow.
pub const FREE_MAX_FOLLOWS: usize = 3;

/// A store subscription is trusted this long after the last time the store confirmed it (offline use).
pub const STORE_GRACE_MS: i64 = 7 * 86_400_000;

/// Public key of the license signature (`license_tool keygen` prints it); replace it with the operator's own.
pub const LICENSE_PUBLIC_KEY: [u8; 32] = [
    0x43, 0x5b, 0x56, 0x5a, 0x21, 0x70, 0x3e, 0x67, 0xca, 0xa5, 0x54, 0x85, 0x49, 0x02, 0xb2, 0x85,
    0x9b, 0xe0, 0x2a, 0xc0, 0xc2, 0x30, 0x19, 0x23, 0xa9, 0xa1, 0xef, 0xd3, 0x01, 0x80, 0x81, 0x38,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Stripe,
    MicrosoftStore,
    AppStore,
    GooglePlay,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Stripe => "Stripe",
            Source::MicrosoftStore => "Microsoft Store",
            Source::AppStore => "App Store",
            Source::GooglePlay => "Google Play",
        }
    }
}

/// What a store reported: a subscription valid until `expires_ms`, confirmed by the store at `checked_ms`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreEntitlement {
    pub source: Source,
    pub expires_ms: i64,
    pub checked_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LicensePayload {
    v: u32,
    /// Free text (buyer, Stripe customer or order id); shown to the user.
    sub: String,
    /// Unix ms; 0 = does not expire.
    exp: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct License {
    pub sub: String,
    pub expires_ms: i64,
}

pub fn verify_license_with(key: &[u8; 32], token: &str, now_ms: i64) -> Result<License, String> {
    let parts: Vec<&str> = token.trim().split('.').collect();
    if parts.len() != 3 || parts[0] != "GIT1" {
        return Err("ライセンスキーの形式が正しくありません".into());
    }
    let payload = B64.decode(parts[1]).map_err(|_| "ライセンスキーを読めません".to_string())?;
    let sig = B64.decode(parts[2]).map_err(|_| "ライセンスキーを読めません".to_string())?;
    let sig = Signature::from_slice(&sig).map_err(|_| "ライセンスキーの署名が正しくありません".to_string())?;
    let vk = VerifyingKey::from_bytes(key).map_err(|e| e.to_string())?;
    vk.verify(&payload, &sig).map_err(|_| "ライセンスキーの署名が正しくありません".to_string())?;
    let p: LicensePayload = serde_json::from_slice(&payload).map_err(|_| "ライセンスキーを読めません".to_string())?;
    if p.v != 1 {
        return Err("このバージョンでは使えないライセンスキーです".into());
    }
    if p.exp != 0 && p.exp < now_ms {
        return Err("ライセンスキーの期限が切れています".into());
    }
    Ok(License { sub: p.sub, expires_ms: p.exp })
}

pub fn verify_license(token: &str, now_ms: i64) -> Result<License, String> {
    verify_license_with(&LICENSE_PUBLIC_KEY, token, now_ms)
}

/// Used by the license tool and the tests: the token for a payload, signed with `secret` (32 bytes).
pub fn sign_license(secret: &[u8; 32], sub: &str, exp_ms: i64) -> String {
    use ed25519_dalek::{Signer, SigningKey};
    let payload = serde_json::to_vec(&LicensePayload { v: 1, sub: sub.into(), exp: exp_ms }).unwrap();
    let sig = SigningKey::from_bytes(secret).sign(&payload);
    format!("GIT1.{}.{}", B64.encode(payload), B64.encode(sig.to_bytes()))
}

/// Days of the free trial (full functions) after the first start. Change it without touching the code: the environment
/// variable `GIT_TRIAL_DAYS` at run time, or at build time (`GIT_TRIAL_DAYS=14 npm run build`); `0` = no trial.
pub const DEFAULT_TRIAL_DAYS: i64 = 7;

pub fn trial_days() -> i64 {
    std::env::var("GIT_TRIAL_DAYS")
        .ok()
        .or_else(|| option_env!("GIT_TRIAL_DAYS").map(String::from))
        .and_then(|v| v.trim().parse::<i64>().ok())
        .map(|d| d.clamp(0, 3650))
        .unwrap_or(DEFAULT_TRIAL_DAYS)
}

/// The plan in force now.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    Free,
    /// The free trial: everything is unlocked until `ends_ms`.
    Trial { ends_ms: i64 },
    Pro { source: Source, expires_ms: i64 },
}

impl Plan {
    /// A paid subscription.
    pub fn is_pro(&self) -> bool {
        matches!(self, Plan::Pro { .. })
    }

    /// No limits apply: paid, or inside the trial.
    pub fn unlocked(&self) -> bool {
        !matches!(self, Plan::Free)
    }
}

pub fn plan(license: &str, store: &Option<StoreEntitlement>, now_ms: i64) -> Plan {
    if let Some(s) = store {
        if s.expires_ms > now_ms && now_ms - s.checked_ms <= STORE_GRACE_MS {
            return Plan::Pro { source: s.source, expires_ms: s.expires_ms };
        }
    }
    if !license.trim().is_empty() {
        if let Ok(l) = verify_license(license, now_ms) {
            return Plan::Pro { source: Source::Stripe, expires_ms: l.expires_ms };
        }
    }
    Plan::Free
}
