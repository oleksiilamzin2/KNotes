# 0027. The chat and its vault tools, in the app and through MCP

Status: accepted, 2026-11-14

## Context

ADR 0004 chose a chat on the owner's ChatGPT plan through Sign in with ChatGPT, with local
function tools over the index, shared with external agents through MCP, and left points to
check. The right panel's "ШІ" segment shows a stub (ADR 0023). The owner asked for the chat
and the tools behind it, and answered a question round (2026-11-14).

Checked in OpenAI's Sign in with ChatGPT documentation (developers.openai.com/siwc, ChatGPT
plan usage for open-source and local apps) on 2026-11-14:

- Authorization: `https://auth.openai.com/api/accounts/authorize` with `response_type=code`,
  PKCE (`S256`), `state`, `nonce`, a loopback `redirect_uri` on `127.0.0.1` (not
  `localhost`; only the port may change), scope `openid profile email offline_access
  resource.invoke chatgpt.tokens.use.direct`, `resource=https://api.openai.com/v1`. The first
  sign-in registers the app: `client_id=dynamic_agent_client`, `agent_name_hint` (the app's
  name), `ext_agent_host_id` (a stable `urn:uuid:` per host); the callback brings the issued
  `client_id` (`oaiapp_…`), saved and used from then on.
- Token endpoint: `https://auth.openai.com/api/accounts/oauth/token`, form-encoded; code
  exchange with the issued `client_id`, `code`, `code_verifier`, `redirect_uri`, `resource`;
  refresh with `grant_type=refresh_token`. Access tokens last an hour; refresh tokens 30
  days, renewed on each refresh (keep the latest, refresh one at a time). Plan usage needs the
  granted `chatgpt.tokens.use.direct` scope; the ID token's issuer is `https://auth.openai.com`
  and its audience the issued client.
- Inference: `POST https://api.openai.com/v1/responses` with the bearer token, `store:false`
  and `stream:true` on every request; the whole conversation goes in `input` each time
  (`previous_response_id` is not supported); no system message items (`instructions` or
  developer messages instead); no `temperature`, `max_output_tokens`, `metadata` and others.
  Models: `GET https://api.openai.com/v1/models`, entries with `visibility == "list"`.
- Tools: function tools work when grouped in a `namespace` (or given in `additional_tools`
  items); hosted tools (file search, code interpreter, image generation, hosted MCP,
  `tool_search`) do not.
- Errors: `subscription_sharing_usage_limit_exceeded` (429: pause, "Manage usage"),
  `…_usage_unavailable` and `…_user_unavailable` (503: retry with backoff), `…_invalid_user`
  (401), `…_user_not_eligible` (403); refresh failures (`invalid_grant`, `refresh_token_expired`,
  `refresh_token_reused`) mean signing in again. Credentials are not dropped for a network
  failure.
- The interface must have a "Continue with ChatGPT" button, a first-use notice ("You're using
  your ChatGPT plan"), "Using ChatGPT plan" with a "Manage usage" link
  (`https://chatgpt.com/settings/usage`) near the composer.

## Decision

- The tools read only (owner's choice): search notes and PDFs (fragments with their place),
  read a note's blocks or a PDF's pages, a document's outline, its links and backlinks, notes
  of a folder or a tag, the tags. Writing is a later stage.
- One tool layer, `knotes-tools`, used two ways: by the in-app chat as namespaced function
  tools, run in the app; and by an MCP server, `knotes-mcp` (stdio), for external agents,
  built in the same stage (owner's choice).
- Conversations: several, listed, kept per vault in Knotes's data beside the index (owner's
  choice); the server keeps none.
- The document open is offered to the conversation (named to the model, readable through the
  tools), shown under the composer, and can be removed (owner's choice).
- Sign-in follows the documentation above. The ID token comes straight from the token
  endpoint over TLS, so its claims (issuer, audience, expiry, nonce) are checked and its
  signature left to TLS, as OpenID Connect allows for that case. Tokens go to the macOS
  Keychain; the host id and the issued client id to Knotes's data.
- The HTTP client is `ureq` (blocking, on threads of the chat's own); streamed events are read
  line by line.

## Consequences

- Fragments the tools find leave the machine with the conversation; the panel says so.
- Usage counts against the owner's ChatGPT plan; the panel links to its usage settings.
- Everything but the live sign-in and the model's answers is tested without the network: a
  transport trait stands in for OpenAI.
