mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use common::Fake;
use knotes_chat::{AccessToken, ChatError, ChatGptAccount, CredentialStore, Credentials, MemoryStore, Registration, SignIn, SignInError, base64url, check_id_token, parse_query, pkce_challenge};
use serde_json::{Value, json};

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// An ID token as the token endpoint gives it (its signature not looked at).
fn id_token(claims: Value) -> String {
    format!("{}.{}.sig", base64url::encode(br#"{"alg":"RS256"}"#), base64url::encode(claims.to_string().as_bytes()))
}

fn claims(client: &str, nonce: &str) -> Value {
    json!({"iss": "https://auth.openai.com", "aud": [client], "sub": "user-1", "email": "me@example.com", "exp": now() + 3600, "nonce": nonce})
}

fn tokens(id_token: &str, scope: &str) -> (u16, String) {
    (200, json!({"access_token": "access-1", "refresh_token": "refresh-1", "id_token": id_token, "token_type": "Bearer", "expires_in": 3600, "scope": scope}).to_string())
}

const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

fn query_of(url: &str) -> Vec<(String, String)> {
    parse_query(url.split_once('?').unwrap().1)
}

fn field<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// The browser coming back to `redirect` with `query`; gives the page it gets.
fn come_back(redirect: &str, query: &str) -> String {
    let address = redirect.trim_start_matches("http://").split('/').next().unwrap().to_string();
    let mut stream = TcpStream::connect(address).unwrap();
    write!(stream, "GET /auth/callback?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
    let mut page = String::new();
    stream.read_to_string(&mut page).unwrap();
    page
}

#[test]
fn pkce_is_sha256_in_base64url_without_padding() {
    assert_eq!(pkce_challenge("abc"), "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0", "FIPS 180-2's SHA-256 of abc");
    assert_eq!(base64url::encode(b"\xfb\xff"), "-_8");
    assert_eq!(base64url::decode("-_8").unwrap(), b"\xfb\xff");
    assert_eq!(base64url::decode("aGk").unwrap(), b"hi");
    assert!(base64url::decode("a$b").is_none());
}

#[test]
fn a_host_gets_one_id_kept_with_its_registration() {
    let dir = tempfile::tempdir().unwrap();
    let first = Registration::load(dir.path());
    assert!(first.host_id.starts_with("urn:uuid:") && first.host_id.len() == 45, "{}", first.host_id);
    first.save(dir.path()).unwrap();
    assert_eq!(Registration::load(dir.path()).host_id, first.host_id);
    assert_ne!(Registration::load(tempfile::tempdir().unwrap().path()).host_id, first.host_id);
}

#[test]
fn a_first_sign_in_registers_the_app_and_a_later_one_uses_the_issued_client() {
    let registration = Registration { host_id: "urn:uuid:host".into(), ..Default::default() };
    let first = SignIn::start(&registration, None).unwrap();
    let query = query_of(&first.url());
    assert!(first.url().starts_with("https://auth.openai.com/api/accounts/authorize?"));
    assert_eq!(field(&query, "client_id"), Some("dynamic_agent_client"));
    assert_eq!(field(&query, "agent_name_hint"), Some("Knotes"));
    assert_eq!(field(&query, "ext_agent_host_id"), Some("urn:uuid:host"));
    assert_eq!(field(&query, "response_type"), Some("code"));
    assert_eq!(field(&query, "scope"), Some(SCOPE));
    assert_eq!(field(&query, "resource"), Some("https://api.openai.com/v1"));
    assert_eq!(field(&query, "code_challenge_method"), Some("S256"));
    assert_eq!(field(&query, "code_challenge").unwrap().len(), 43);
    assert!(field(&query, "state").unwrap().len() >= 43 && field(&query, "nonce").unwrap().len() >= 43);
    let redirect = field(&query, "redirect_uri").unwrap();
    assert!(redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/auth/callback"), "{redirect}");

    let registration = Registration { host_id: "urn:uuid:host".into(), client_id: Some("oaiapp_1".into()), email: Some("me@example.com".into()), ..Default::default() };
    let again = SignIn::start(&registration, Some("old.id.token")).unwrap();
    let query = query_of(&again.url());
    assert_eq!(field(&query, "client_id"), Some("oaiapp_1"));
    assert_eq!(field(&query, "agent_name_hint"), None);
    assert_eq!(field(&query, "id_token_hint"), Some("old.id.token"));
    assert_eq!(field(&query, "login_hint"), Some("me@example.com"));
    assert_ne!(field(&query, "state"), field(&query_of(&first.url()), "state"), "fresh values each time");
}

#[test]
fn the_browser_s_return_is_exchanged_for_checked_tokens() {
    let registration = Registration { host_id: "urn:uuid:host".into(), ..Default::default() };
    let sign_in = SignIn::start(&registration, None).unwrap();
    let query = query_of(&sign_in.url());
    let (redirect, state, nonce) = (field(&query, "redirect_uri").unwrap().to_string(), field(&query, "state").unwrap().to_string(), field(&query, "nonce").unwrap().to_string());
    let verifier_challenge = field(&query, "code_challenge").unwrap().to_string();
    let browser = std::thread::spawn(move || {
        let stray = come_back(&redirect, "code=x&state=forged&client_id=oaiapp_1");
        let page = come_back(&redirect, &format!("code=code-1&scope=openid&state={state}&client_id=oaiapp_1"));
        (stray, page)
    });
    let fake = Fake::answering(vec![tokens(&id_token(claims("oaiapp_1", &nonce)), SCOPE)]);
    let credentials = sign_in.complete(&fake, &AtomicBool::new(false)).unwrap();
    let (stray, page) = browser.join().unwrap();
    assert!(stray.starts_with("HTTP/1.1 400"), "a forged state is turned away: {stray}");
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("Knotes"), "{page}");

    assert_eq!(credentials.client_id, "oaiapp_1");
    assert_eq!((credentials.subject.as_str(), credentials.email.as_deref()), ("user-1", Some("me@example.com")));
    assert_eq!((credentials.access_token.as_str(), credentials.refresh_token.as_str()), ("access-1", "refresh-1"));
    assert!(credentials.expires_at >= now() + 3590);
    let request = &fake.sent.borrow()[0];
    assert_eq!(request.url, "https://auth.openai.com/api/accounts/oauth/token");
    assert_eq!(fake.header(0, "Content-Type"), "application/x-www-form-urlencoded");
    let form = fake.form(0);
    assert_eq!(field(&form, "grant_type"), Some("authorization_code"));
    assert_eq!(field(&form, "client_id"), Some("oaiapp_1"));
    assert_eq!(field(&form, "code"), Some("code-1"));
    assert_eq!(field(&form, "resource"), Some("https://api.openai.com/v1"));
    assert_eq!(field(&form, "redirect_uri"), field(&query, "redirect_uri"));
    assert_eq!(pkce_challenge(field(&form, "code_verifier").unwrap()), verifier_challenge);
}

#[test]
fn a_sign_in_turned_down_or_without_the_plan_says_so() {
    let registration = Registration { host_id: "urn:uuid:host".into(), ..Default::default() };
    let sign_in = SignIn::start(&registration, None).unwrap();
    let query = query_of(&sign_in.url());
    let (redirect, state) = (field(&query, "redirect_uri").unwrap().to_string(), field(&query, "state").unwrap().to_string());
    std::thread::spawn(move || come_back(&redirect, &format!("error=access_denied&state={state}")));
    assert_eq!(sign_in.complete(&Fake::default(), &AtomicBool::new(false)).unwrap_err(), SignInError::Declined);

    let sign_in = SignIn::start(&registration, None).unwrap();
    let query = query_of(&sign_in.url());
    let (redirect, state, nonce) = (field(&query, "redirect_uri").unwrap().to_string(), field(&query, "state").unwrap().to_string(), field(&query, "nonce").unwrap().to_string());
    std::thread::spawn(move || come_back(&redirect, &format!("code=c&state={state}&client_id=oaiapp_1")));
    let fake = Fake::answering(vec![tokens(&id_token(claims("oaiapp_1", &nonce)), "openid profile email offline_access")]);
    assert_eq!(sign_in.complete(&fake, &AtomicBool::new(false)).unwrap_err(), SignInError::PlanNotEnabled);

    let sign_in = SignIn::start(&registration, None).unwrap();
    assert_eq!(sign_in.complete(&Fake::default(), &AtomicBool::new(true)).unwrap_err(), SignInError::Cancelled);
}

#[test]
fn an_id_token_is_checked_for_its_issuer_audience_expiry_and_nonce() {
    let good = claims("oaiapp_1", "n1");
    assert_eq!(check_id_token(&id_token(good.clone()), "oaiapp_1", "n1", now()).unwrap().subject, "user-1");
    let single_audience = json!({"aud": "oaiapp_1"});
    let mut with = good.clone();
    with.as_object_mut().unwrap().extend(single_audience.as_object().unwrap().clone());
    assert!(check_id_token(&id_token(with), "oaiapp_1", "n1", now()).is_ok());
    for (key, value) in [("iss", json!("https://evil.example")), ("aud", json!(["oaiapp_2"])), ("exp", json!(now() - 10)), ("nonce", json!("n2")), ("sub", json!(""))] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(check_id_token(&id_token(bad), "oaiapp_1", "n1", now()).is_err(), "{key}");
    }
    assert!(check_id_token("not-a-token", "oaiapp_1", "n1", now()).is_err());
}

fn signed_in(expires_at: u64) -> Credentials {
    Credentials {
        client_id: "oaiapp_1".into(),
        subject: "user-1".into(),
        email: Some("me@example.com".into()),
        id_token: "id-1".into(),
        access_token: "access-1".into(),
        refresh_token: "refresh-1".into(),
        expires_at,
        scopes: vec!["chatgpt.tokens.use.direct".into()],
    }
}

#[test]
fn credentials_survive_their_json() {
    let credentials = signed_in(1234);
    assert_eq!(Credentials::from_json(&credentials.to_json()), Some(credentials));
}

#[test]
fn a_token_near_its_end_is_refreshed_and_the_new_refresh_token_kept() {
    let store = MemoryStore::default();
    store.save(&signed_in(now() + 60)).unwrap();
    let fake = Fake::answering(vec![(200, json!({"access_token": "access-2", "refresh_token": "refresh-2", "expires_in": 3600, "scope": SCOPE}).to_string())]);
    let account = ChatGptAccount::new(fake, store);
    assert_eq!(account.token(), Ok("access-2".into()));
    assert_eq!(account.token(), Ok("access-2".into()), "refreshed once");
    let form = account.transport().form(0);
    assert_eq!(field(&form, "grant_type"), Some("refresh_token"));
    assert_eq!(field(&form, "client_id"), Some("oaiapp_1"));
    assert_eq!(field(&form, "refresh_token"), Some("refresh-1"));
    assert_eq!(field(&form, "resource"), Some("https://api.openai.com/v1"));
    assert_eq!(field(&form, "scope"), None);
    let kept = account.store().load().unwrap();
    assert_eq!((kept.access_token.as_str(), kept.refresh_token.as_str(), kept.id_token.as_str()), ("access-2", "refresh-2", "id-1"));
    assert!(kept.expires_at >= now() + 3590);
}

#[test]
fn a_refused_refresh_signs_out_and_a_network_failure_keeps_the_tokens() {
    let store = MemoryStore::default();
    store.save(&signed_in(now() + 3600)).unwrap();
    let fake = Fake::default();
    fake.answers.borrow_mut().push_back(Err(ChatError::Network("offline".into())));
    fake.answers.borrow_mut().push_back(Ok((400, json!({"error": "invalid_grant", "error_description": "reused"}).to_string())));
    let account = ChatGptAccount::new(fake, store);
    assert_eq!(account.token(), Ok("access-1".into()), "still fresh: no refresh");
    assert_eq!(account.renew(), Err(ChatError::Network("offline".into())));
    assert!(account.store().load().is_some(), "kept for a network failure");
    assert!(matches!(account.renew(), Err(ChatError::SignInAgain(_))));
    assert!(account.store().load().is_none());
    assert_eq!(account.email(), None);
    assert!(matches!(account.token(), Err(ChatError::SignInAgain(_))));
}

#[test]
fn signing_in_and_out_changes_the_account_kept() {
    let account = ChatGptAccount::new(Fake::answering(vec![(200, json!({"revocation_endpoint": "https://auth.openai.com/oauth/revoke"}).to_string()), (200, String::new())]), MemoryStore::default());
    assert_eq!(account.email(), None);
    account.accept(signed_in(now() + 3600)).unwrap();
    assert_eq!(account.email(), Some("me@example.com".into()));
    assert!(account.store().load().is_some());
    account.sign_out();
    assert_eq!(account.email(), None);
    assert!(account.store().load().is_none());
    let form = account.transport().form(1);
    assert_eq!(account.transport().sent.borrow()[1].url, "https://auth.openai.com/oauth/revoke");
    assert_eq!((field(&form, "token"), field(&form, "token_type_hint"), field(&form, "client_id")), (Some("refresh-1"), Some("refresh_token"), Some("oaiapp_1")));
}
