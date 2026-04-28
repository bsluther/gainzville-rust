# Authentication Design

## Approach: Sign in with Apple + Own Session Tokens

Gainzville uses **Sign in with Apple (SIWA)** as the sole identity provider for V1. Apple verifies who the user is; Gainzville issues its own short-lived access tokens and long-lived refresh tokens for subsequent sessions. No passwords are stored.

This is the standard pattern used by most serious iOS apps: OAuth/OIDC for the initial identity assertion, then your own session layer for everything after. The two concerns are cleanly separated.

---

## Cryptographic Primitives

Understanding the four cryptographic building blocks used makes it easy to reason about what each step is actually guaranteeing.

### 1. RS256 — Apple's JWT signatures

Apple signs its identity tokens using **RS256**: RSA with SHA-256. RSA is an asymmetric algorithm — Apple holds a private key and publishes corresponding public keys at a well-known URL. Anyone with the public key can verify a signature but cannot forge one. The signature covers the base64url-encoded header and payload of the JWT, so tampering with any claim invalidates it.

Apple rotates its signing keys periodically and publishes them in JWKS (JSON Web Key Set) format. Each key has a `kid` (key ID) field; the JWT header names which key was used, so the verifier knows which public key to fetch.

### 2. ES256 — Our own JWT signatures

For the access tokens Gainzville issues, we use **ES256**: ECDSA with P-256 and SHA-256. This is also asymmetric — the server holds a private key, the client (Swift app) has the public key embedded at build time or fetched on first run. ES256 produces smaller keys and signatures than RS256 with equivalent security, which matters for tokens that travel on every request.

The signature is computed over `base64url(header) + "." + base64url(payload)`. Verifying it locally (without a network call) is what enables the offline-first access token check.

### 3. SHA-256 — Nonce binding and refresh token storage

SHA-256 is a one-way hash function. Two specific uses here:

**Nonce binding**: before initiating login, the Swift app generates a random nonce and sends `SHA-256(nonce)` to Apple. Apple embeds this hash in the identity token. Our server verifies `SHA-256(raw_nonce) == nonce_claim`. This prevents a stolen authorization response from being replayed — an attacker who intercepts the identity token cannot use it because they don't know the raw nonce that was only ever in the originating app.

**Refresh token storage**: the server stores `SHA-256(refresh_token)` rather than the raw token. If the database is compromised, the attacker gets hashes, not valid tokens. SHA-256 (not argon2) is appropriate here because refresh tokens are already 32 random bytes — there is no dictionary to attack. Argon2 is for passwords, which are low-entropy human-chosen strings.

### 4. CSPRNG — Random token generation

Refresh tokens are 32 bytes from a cryptographically secure pseudorandom number generator (OS-level `/dev/urandom` via Rust's `rand::thread_rng` or `getrandom`). This gives 256 bits of entropy, making brute-force search computationally infeasible. The token is base64url-encoded for transport.

---

## Flow 1: First Login / Signup

This is the only flow that requires both network connectivity and Apple's servers.

```
Swift App                          Apple                      Gainzville Server
    |                                 |                               |
    |  1. Generate nonce (32 bytes)   |                               |
    |  2. Compute SHA-256(nonce)      |                               |
    |  3. ASAuthorizationController   |                               |
    |     .requestedScopes: [.email]  |                               |
    |     .nonce: SHA-256(nonce) ─────►                               |
    |                                 |  (user authenticates)         |
    |◄── identity_token (JWT) ────────|                               |
    |    authorization_code           |                               |
    |                                 |                               |
    |  POST /auth/apple ─────────────────────────────────────────────►
    |  { identity_token,              |                               |
    |    authorization_code,          |                               |
    |    raw_nonce }                  |                               |
    |                                 |                               |
    |                                 |  4. Fetch Apple JWKS          |
    |                                 |◄──────────────────────────────|
    |                                 |──────────────────────────────►|
    |                                 |                               |
    |                                 |  5. Verify identity_token:    |
    |                                 |     - RS256 sig valid?        |
    |                                 |     - iss == appleid.apple.com|
    |                                 |     - aud == bundle ID        |
    |                                 |     - exp not passed          |
    |                                 |     - nonce == SHA-256(raw)?  |
    |                                 |                               |
    |                                 |  6. Extract sub (Apple user ID|
    |                                 |  7. Upsert user record        |
    |                                 |  8. Issue access_token (JWT)  |
    |                                 |     ES256, exp = now + 30min  |
    |                                 |  9. Issue refresh_token       |
    |                                 |     32 random bytes           |
    |                                 |     store SHA-256(token) in DB|
    |                                 |                               |
    |◄── { access_token,             |                               |
    |      refresh_token,             |                               |
    |      user_id,                   |                               |
    |      refresh_token_exp }        |                               |
    |                                 |                               |
    |  10. Store all four in Keychain |                               |
    |  11. Init GainzvilleCore(user_id)|                              |
```

**Step 5 in detail** — what "verify the JWT" means concretely:

1. Base64url-decode the header. Read `kid` (which Apple key to use) and confirm `alg == RS256`.
2. Fetch the JWKS document from Apple (cache it; Apple rarely rotates keys). Find the key matching `kid`. Reconstruct the RSA public key from the `n` (modulus) and `e` (exponent) fields.
3. Verify the ECDSA/RSA signature over `header_b64 + "." + payload_b64`.
4. Base64url-decode the payload. Check each claim:
   - `iss`: must be `https://appleid.apple.com`
   - `aud`: must be your app's bundle ID (prevents tokens issued for other apps from working here)
   - `exp`: must be in the future (prevents replay of old tokens)
   - `nonce`: must equal `SHA-256(raw_nonce)` the client sent (prevents stolen-token replay)
5. If all checks pass, the `sub` claim is a verified, stable identifier for this Apple user.

---

## Flow 2: App Restart (Tokens Already Stored)

No network required if the access token is still valid.

```
App Launch
    |
    |  1. Read tokens from Keychain
    |     - access_token present?  → yes/no
    |     - refresh_token present? → yes/no
    |
    ├── No tokens → show login screen
    |
    ├── access_token present, exp > now
    |      → Init GainzvilleCore(user_id), proceed normally
    |
    └── access_token expired, refresh_token present, refresh_token_exp > now
           → Flow 3 (token refresh)
```

The `exp` check on the access token is a local operation: base64url-decode the payload, parse the `exp` unix timestamp, compare to `Date.now()`. No network, no server call.

---

## Flow 3: Token Refresh

Happens silently when the access token expires. Network required, but only with Gainzville's server — not Apple.

```
Swift App                          Gainzville Server
    |                                     |
    |  POST /auth/refresh ────────────────►
    |  { refresh_token }                  |
    |                                     |
    |                                     |  1. Compute SHA-256(refresh_token)
    |                                     |  2. Look up matching row in refresh_sessions
    |                                     |  3. Check expires_at > now
    |                                     |  4. Check not revoked
    |                                     |  5. Issue new access_token (JWT, exp = now + 30min)
    |                                     |  6. Optionally rotate refresh_token
    |                                     |     (sliding window: extend expiry on use)
    |                                     |
    |◄── { access_token [, refresh_token]}|
    |                                     |
    |  Update Keychain                    |
```

**Offline grace period**: if the network call fails with a timeout or connection error (not a 401/403 from the server), the app allows continued local use. The logic:

```swift
if networkError && Date() < refreshTokenExp {
    // allow offline use — user is still within their session window
    continueWithCurrentUser()
} else if serverReturned401 {
    // token was revoked server-side — force logout
    logout()
}
```

This means a user can take their device offline indefinitely (up to the refresh token lifetime) without being kicked out. They cannot sync, but all local reads and writes work normally.

---

## Flow 4: Logout

```
Swift App                          Gainzville Server
    |                                     |
    |  POST /auth/logout ─────────────────►
    |  Authorization: Bearer <access_token>|
    |  { refresh_token }                  |
    |                                     |
    |                                     |  1. Delete refresh_sessions row
    |                                     |     (or mark revoked)
    |                                     |
    |◄── 204 No Content                   |
    |                                     |
    |  Delete all Keychain items          |
    |  Tear down GainzvilleCore instance  |
    |  Show login screen                  |
```

The access token cannot be revoked (it's stateless). This is acceptable because it expires in 30 minutes regardless. If you need hard revocation (e.g. "sign out all devices" or a compromised account), maintain a `revoked_jtis` table with a TTL equal to the access token lifetime — the JWT middleware checks it on every request. For V1 this is optional.

**Offline logout**: if the server call fails, clear Keychain and tear down the core instance locally anyway. The refresh token will be cleaned up server-side on the next connection, or will expire naturally.

---

## How Identity Flows into Gainzville Core

The key design constraint: **JWTs and tokens never cross the FFI boundary**. Core and the SQLite client operate on `ActorId` (UUID), not tokens. Token verification is exclusively a server/network concern.

```
Keychain
  └── user_id (UUID string)
        │
        ▼
GainzvilleCore::new(db_path, actor_id)    ← FFI boundary
        │
        ▼
Action { actor_id, ... }
        │
        ▼
Mutator → checks actor_id == owner_id     ← existing auth logic, unchanged
        │
        ▼
SqliteApply                               ← no token awareness needed
```

For server-side requests, the JWT middleware extracts `sub` from the access token, constructs an `Actor::User(user_id)`, and passes it through the same action/query pipeline. The pipeline itself doesn't know or care whether the identity came from a local Keychain or a verified JWT.

This means the existing ownership checks in `mutators.rs` work identically for offline local writes and server-side sync writes.

---

## Server Endpoints Required

```
POST /auth/apple     { identity_token, authorization_code, raw_nonce, device_name? }
                     → { access_token, refresh_token, user_id, refresh_token_exp }

POST /auth/refresh   { refresh_token }
                     → { access_token }           (or { access_token, refresh_token } if rotating)

POST /auth/logout    { refresh_token }  + Authorization: Bearer <access_token>
                     → 204

DELETE /auth/sessions  + Authorization: Bearer <access_token>
                     → 204  (revoke all sessions for this user — "sign out everywhere")
```

---

## Database Schema Additions

These live in the `server` crate migrations, not in `core`. Core's `actors`/`users` tables are unchanged.

```sql
-- Stable Apple identity → internal user mapping
CREATE TABLE apple_identities (
    apple_subject_id TEXT PRIMARY KEY,   -- Apple's stable `sub` claim
    actor_id         UUID NOT NULL REFERENCES actors(id),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One row per active device session
CREATE TABLE refresh_sessions (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash       TEXT NOT NULL UNIQUE,  -- SHA-256(refresh_token), hex-encoded
    actor_id         UUID NOT NULL REFERENCES actors(id),
    device_name      TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at       TIMESTAMPTZ NOT NULL,
    last_used_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked          BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX refresh_sessions_actor_id ON refresh_sessions(actor_id);
```

No password hashes, no email verification state — Apple handles all of that.

---

## Rust Crate Dependencies

```toml
# In server/Cargo.toml or a new gv-auth crate
jsonwebtoken = "9"     # JWT encode/decode, RS256 + ES256 support
p256 = "0.13"          # P-256 key generation for our own ES256 tokens
                       # (or use jsonwebtoken's built-in via openssl/ring)
reqwest = "0.12"       # Fetch Apple's JWKS endpoint
rand = "0.8"           # CSPRNG for refresh token generation
sha2 = "0.10"          # SHA-256 for nonce verification and token hashing
axum = "0.7"           # HTTP server (not yet in workspace)
```

---

## What Remains for Authorization

Authentication (who are you?) is handled by the above. Authorization (what can you do?) is separate and already partially implemented:

- **Write path**: `mutators.rs` already checks `actor_id == owner_id` for all mutations. This continues to work unchanged.
- **Read path**: queries currently have no actor context. The `permissions.md` doc notes this as a known gap. Adding `actor_id` to query types is the next authorization milestone after authentication is shipping.

The auth token from login slots directly into the permissions model described in `docs/permissions.md` once queries carry actor context.
