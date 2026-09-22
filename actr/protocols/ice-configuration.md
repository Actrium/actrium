# Actr ICE Configuration

Language: **English** | [简体中文](ice-configuration.zh.md)

Status: initial `actr-ice-config/1` draft.

The `ice-config` service relation points to an HTTP endpoint from which an Actr Host
obtains the STUN and TURN configuration for its authority. The public authority
descriptor contains only this endpoint; it never embeds TURN passwords.

## Request and response

The service supports `GET` with `Accept: application/json`. Production clients
use HTTPS. A successful response conforms to
[`ice-configuration.schema.json`](../../contracts/actr/ice-configuration.schema.json):

```json
{
  "version": 1,
  "ice_servers": [
    {
      "urls": ["stun:rtc.example.com:3478"]
    },
    {
      "urls": ["turn:rtc.example.com:3478"],
      "username": "short-lived-username",
      "credential": "short-lived-credential"
    }
  ]
}
```

An anonymous response may publish STUN URLs and TURN locations without a
credential. An Actr Host must authenticate through the authority's identity flow
before using TURN when credentials are required. Credentials returned here are
short-lived and the response must use private, correspondingly short caching.

Clients ignore unknown members, reject unsupported versions, and apply normal
WebRTC ICE URL, transport, and local network policy. The endpoint must not issue
credentials solely because the caller knows an `actr:` URI.

Loran currently exposes its configured STUN/TURN locations at this endpoint;
Issuer registration and renewal remain the authority for Actor-bound, time-limited
TURN credentials.
