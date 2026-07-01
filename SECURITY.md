# Security & deployment

Read Flow authenticates with HTTP Basic (Argon2id-hashed passwords), then hands
out a short-lived Bearer JWT for subsequent requests. **The transport is not
encrypted by default** — you must put TLS in front of the server before exposing
it beyond `localhost`.

## Why TLS is required off-localhost

Without TLS, Basic credentials, the Bearer token, and file contents cross the
network in the clear and can be read or altered by anyone on the path. In
addition, a PWA served over HTTPS **cannot** talk to an HTTP API (browsers block
mixed content), so the API must be HTTPS whenever the PWA isn't on `localhost`.

Serving on `127.0.0.1` for same-machine use (e.g. the COSMIC app's embedded
server) needs no TLS.

## Pick one TLS option

### A. Built-in HTTPS (rustls) — integrated, no extra software

Point the server at a certificate and key in `read-flow.toml`:

```toml
[server.tls]
cert = "/path/to/fullchain.pem"
key  = "/path/to/privkey.pem"
```

The server then speaks HTTPS (and sends HSTS). Get a certificate from:
- **Let's Encrypt** (e.g. `certbot certonly`) if you have a public domain — a
  publicly-trusted cert, clients install nothing.
- **A self-signed cert** for a private LAN — works for the COSMIC desktop
  client, but browsers/PWAs will reject it, so it's not suitable for the PWA.

Operator installs nothing extra; clients install nothing *if* the cert is
publicly trusted.

### B. Reverse proxy with automatic HTTPS (Caddy) — recommended with a domain

Keep Read Flow on plain HTTP bound to localhost and run Caddy in front. Needs a
domain pointed at the host and ports 80/443 reachable. Caddy obtains and renews
Let's Encrypt certs automatically; clients install nothing.

`Caddyfile`:

```
read-flow.example.com {
    reverse_proxy 127.0.0.1:8000
}
```

```toml
# read-flow.toml
[server]
address = "127.0.0.1"
port = 8000
allowed_origins = ["https://read-flow.example.com"]
```

### C. Tailscale / WireGuard — no domain, no open ports

Install Tailscale on the server and on each client device. The mesh encrypts and
authenticates all traffic, so you can keep the API on HTTP over the tailnet
without a public domain or exposed ports. (Optionally `tailscale cert` gives you
a real HTTPS cert on your tailnet domain for use with option A.) Each device
needs the Tailscale client installed.

## Other hardening in place

- **Argon2id** password hashing (legacy PBKDF2 hashes still verify).
- **Bearer JWT** exchange so PBKDF2/Argon2 runs once per session, not per call.
- **Constant-time** auth on unknown users (no username enumeration via timing).
- **CORS** restricted via `[server].allowed_origins` (any origin + a warning when
  unset).
- **Upload size limit** via `[server].max_upload_bytes` (default 100 MiB).
- **Rate limiting** on `/oauth/token`.
- Security headers: `Strict-Transport-Security` (with TLS),
  `X-Content-Type-Options`, `Referrer-Policy`.
- Clients warn when about to send credentials over plaintext HTTP to a
  non-loopback host.

## Known residual risks (not yet addressed)

- **Credentials at rest are stored in plaintext**: remote passphrases in the
  PWA's IndexedDB and the COSMIC app's SQLite `remotes` table. Protect the device
  / disk; a future change should move these to an OS keyring / Web Crypto or store
  only short-lived tokens.
- **Token revocation is coarse**: the JWT signing secret is in-memory and
  ephemeral, so restarting the server invalidates all tokens; there is no
  per-token revocation or refresh yet.
