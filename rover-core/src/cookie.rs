use mlua::prelude::*;
use std::collections::HashMap;

/// Cookie attribute builder for setting cookies with secure attributes
#[derive(Debug, Clone, Default)]
pub struct CookieBuilder {
    name: String,
    value: String,
    path: Option<String>,
    domain: Option<String>,
    max_age: Option<i64>,
    expires: Option<String>,
    secure: bool,
    http_only: bool,
    same_site: Option<SameSite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl SameSite {
    pub fn as_str(&self) -> &'static str {
        match self {
            SameSite::Strict => "Strict",
            SameSite::Lax => "Lax",
            SameSite::None => "None",
        }
    }
}

impl CookieBuilder {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            ..Default::default()
        }
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn max_age(mut self, seconds: i64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    pub fn expires(mut self, date: impl Into<String>) -> Self {
        self.expires = Some(date.into());
        self
    }

    pub fn secure(mut self) -> Self {
        self.secure = true;
        self
    }

    pub fn http_only(mut self) -> Self {
        self.http_only = true;
        self
    }

    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }

    pub fn build(self) -> String {
        let mut cookie = format!("{}={}", self.name, self.value);

        if let Some(path) = self.path {
            cookie.push_str(&format!("; Path={}", path));
        }

        if let Some(domain) = self.domain {
            cookie.push_str(&format!("; Domain={}", domain));
        }

        if let Some(max_age) = self.max_age {
            cookie.push_str(&format!("; Max-Age={}", max_age));
        }

        if let Some(expires) = self.expires {
            cookie.push_str(&format!("; Expires={}", expires));
        }

        if self.secure {
            cookie.push_str("; Secure");
        }

        if self.http_only {
            cookie.push_str("; HttpOnly");
        }

        if let Some(same_site) = self.same_site {
            cookie.push_str(&format!("; SameSite={}", same_site.as_str()));
        }

        cookie
    }
}

/// Parse cookies from a Cookie header value
pub fn parse_cookies(header_value: &str) -> HashMap<String, String> {
    let mut cookies = HashMap::new();

    for cookie in header_value.split(';') {
        let cookie = cookie.trim();
        if let Some((name, value)) = cookie.split_once('=') {
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if !name.is_empty() {
                cookies.insert(name, value);
            }
        }
    }

    cookies
}

/// Parse a single Set-Cookie header value into its components
pub fn parse_set_cookie(header_value: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut parts = header_value.split(';');

    // First part is always name=value
    if let Some(first) = parts.next()
        && let Some((name, value)) = first.split_once('=')
    {
        result.insert("name".to_string(), name.trim().to_string());
        result.insert("value".to_string(), value.trim().to_string());
    }

    // Remaining parts are attributes
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            result.insert(key, value);
        } else {
            // Boolean attributes like Secure, HttpOnly
            let key = part.trim().to_lowercase();
            result.insert(key, "true".to_string());
        }
    }

    result
}

/// Create a deletion cookie (expires in the past)
pub fn delete_cookie(name: impl Into<String>, path: Option<&str>, domain: Option<&str>) -> String {
    let mut builder = CookieBuilder::new(name, "")
        .max_age(0)
        .expires("Thu, 01 Jan 1970 00:00:00 GMT");

    if let Some(path) = path {
        builder = builder.path(path);
    }

    if let Some(domain) = domain {
        builder = builder.domain(domain);
    }

    builder.build()
}

/// Lua userdata wrapper for CookieBuilder
#[derive(Clone)]
struct LuaCookieBuilder(CookieBuilder);

impl LuaUserData for LuaCookieBuilder {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("path", |_lua, this, path: String| {
            Ok(LuaCookieBuilder(this.0.clone().path(path)))
        });

        methods.add_method("domain", |_lua, this, domain: String| {
            Ok(LuaCookieBuilder(this.0.clone().domain(domain)))
        });

        methods.add_method("max_age", |_lua, this, seconds: i64| {
            Ok(LuaCookieBuilder(this.0.clone().max_age(seconds)))
        });

        methods.add_method("expires", |_lua, this, date: String| {
            Ok(LuaCookieBuilder(this.0.clone().expires(date)))
        });

        methods.add_method("secure", |_lua, this, ()| {
            Ok(LuaCookieBuilder(this.0.clone().secure()))
        });

        methods.add_method("http_only", |_lua, this, ()| {
            Ok(LuaCookieBuilder(this.0.clone().http_only()))
        });

        methods.add_method("same_site", |_lua, this, value: String| {
            let same_site = match value.to_lowercase().as_str() {
                "strict" => SameSite::Strict,
                "lax" => SameSite::Lax,
                "none" => SameSite::None,
                _ => SameSite::Lax,
            };
            Ok(LuaCookieBuilder(this.0.clone().same_site(same_site)))
        });

        methods.add_method("build", |_lua, this, ()| Ok(this.0.clone().build()));
    }
}

pub fn create_cookie_module(lua: &Lua) -> LuaResult<LuaTable> {
    let cookie = lua.create_table()?;

    // Parse cookies from Cookie header
    cookie.set(
        "parse",
        lua.create_function(|lua, header_value: String| {
            let cookies = parse_cookies(&header_value);
            let table = lua.create_table()?;
            for (name, value) in cookies {
                table.set(name, value)?;
            }
            Ok(table)
        })?,
    )?;

    // Parse Set-Cookie header
    cookie.set(
        "parse_set_cookie",
        lua.create_function(|lua, header_value: String| {
            let parsed = parse_set_cookie(&header_value);
            let table = lua.create_table()?;
            for (key, value) in parsed {
                table.set(key, value)?;
            }
            Ok(table)
        })?,
    )?;

    // Create a new cookie builder
    cookie.set(
        "set",
        lua.create_function(|_lua, (name, value): (String, String)| {
            Ok(LuaCookieBuilder(CookieBuilder::new(name, value)))
        })?,
    )?;

    // Delete a cookie
    cookie.set(
        "delete",
        lua.create_function(|_lua, (name, opts): (String, Option<LuaTable>)| {
            let path = opts.as_ref().and_then(|t| t.get::<String>("path").ok());
            let domain = opts.as_ref().and_then(|t| t.get::<String>("domain").ok());

            let deletion_cookie = delete_cookie(name, path.as_deref(), domain.as_deref());

            Ok(deletion_cookie)
        })?,
    )?;

    Ok(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_simple_cookie_header() {
        let header = "session=abc123; user=john";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("user"), Some(&"john".to_string()));
    }

    #[test]
    fn should_parse_single_cookie() {
        let header = "token=xyz789";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("token"), Some(&"xyz789".to_string()));
        assert_eq!(cookies.len(), 1);
    }

    #[test]
    fn should_handle_empty_cookie_header() {
        let cookies = parse_cookies("");
        assert!(cookies.is_empty());
    }

    #[test]
    fn should_handle_whitespace_in_cookie_header() {
        let header = "  session = abc123  ;  user = john  ";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("user"), Some(&"john".to_string()));
    }

    #[test]
    fn should_build_simple_cookie() {
        let cookie = CookieBuilder::new("session", "abc123").build();
        assert_eq!(cookie, "session=abc123");
    }

    #[test]
    fn should_build_cookie_with_path() {
        let cookie = CookieBuilder::new("session", "abc123").path("/api").build();
        assert_eq!(cookie, "session=abc123; Path=/api");
    }

    #[test]
    fn should_build_cookie_with_domain() {
        let cookie = CookieBuilder::new("session", "abc123")
            .domain("example.com")
            .build();
        assert_eq!(cookie, "session=abc123; Domain=example.com");
    }

    #[test]
    fn should_build_secure_cookie() {
        let cookie = CookieBuilder::new("session", "abc123").secure().build();
        assert_eq!(cookie, "session=abc123; Secure");
    }

    #[test]
    fn should_build_http_only_cookie() {
        let cookie = CookieBuilder::new("session", "abc123").http_only().build();
        assert_eq!(cookie, "session=abc123; HttpOnly");
    }

    #[test]
    fn should_build_cookie_with_max_age() {
        let cookie = CookieBuilder::new("session", "abc123")
            .max_age(3600)
            .build();
        assert_eq!(cookie, "session=abc123; Max-Age=3600");
    }

    #[test]
    fn should_build_cookie_with_expires() {
        let cookie = CookieBuilder::new("session", "abc123")
            .expires("Wed, 21 Oct 2025 07:28:00 GMT")
            .build();
        assert_eq!(
            cookie,
            "session=abc123; Expires=Wed, 21 Oct 2025 07:28:00 GMT"
        );
    }

    #[test]
    fn should_build_cookie_with_same_site_strict() {
        let cookie = CookieBuilder::new("session", "abc123")
            .same_site(SameSite::Strict)
            .build();
        assert_eq!(cookie, "session=abc123; SameSite=Strict");
    }

    #[test]
    fn should_build_cookie_with_same_site_lax() {
        let cookie = CookieBuilder::new("session", "abc123")
            .same_site(SameSite::Lax)
            .build();
        assert_eq!(cookie, "session=abc123; SameSite=Lax");
    }

    #[test]
    fn should_build_cookie_with_same_site_none() {
        let cookie = CookieBuilder::new("session", "abc123")
            .same_site(SameSite::None)
            .build();
        assert_eq!(cookie, "session=abc123; SameSite=None");
    }

    #[test]
    fn should_build_cookie_with_all_attributes() {
        let cookie = CookieBuilder::new("session", "abc123")
            .path("/")
            .domain("example.com")
            .max_age(3600)
            .secure()
            .http_only()
            .same_site(SameSite::Strict)
            .build();

        assert!(cookie.contains("session=abc123"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Domain=example.com"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
    }

    #[test]
    fn should_create_delete_cookie() {
        let cookie = delete_cookie("session", Some("/"), None);
        assert!(cookie.contains("session="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Expires=Thu, 01 Jan 1970 00:00:00 GMT"));
        assert!(cookie.contains("Path=/"));
    }

    #[test]
    fn should_create_delete_cookie_with_domain() {
        let cookie = delete_cookie("session", Some("/api"), Some("example.com"));
        assert!(cookie.contains("session="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Path=/api"));
        assert!(cookie.contains("Domain=example.com"));
    }

    #[test]
    fn should_parse_set_cookie_header() {
        let header = "session=abc123; Path=/; Secure; HttpOnly; SameSite=Strict";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("name"), Some(&"session".to_string()));
        assert_eq!(parsed.get("value"), Some(&"abc123".to_string()));
        assert_eq!(parsed.get("path"), Some(&"/".to_string()));
        assert_eq!(parsed.get("secure"), Some(&"true".to_string()));
        assert_eq!(parsed.get("httponly"), Some(&"true".to_string()));
        assert_eq!(parsed.get("samesite"), Some(&"Strict".to_string()));
    }

    #[test]
    fn should_parse_set_cookie_with_max_age() {
        let header = "token=xyz; Max-Age=3600; Path=/api";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("name"), Some(&"token".to_string()));
        assert_eq!(parsed.get("value"), Some(&"xyz".to_string()));
        assert_eq!(parsed.get("max-age"), Some(&"3600".to_string()));
        assert_eq!(parsed.get("path"), Some(&"/api".to_string()));
    }

    #[test]
    fn should_handle_cookie_value_with_equals() {
        let header = "data=key=value";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("data"), Some(&"key=value".to_string()));
    }

    #[test]
    fn should_handle_url_encoded_cookie_values() {
        let header = "data=hello%20world";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("data"), Some(&"hello%20world".to_string()));
    }

    // Cookie parse/serialize edge cases

    #[test]
    fn should_handle_cookie_with_empty_value() {
        let header = "session=; user=john";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("session"), Some(&"".to_string()));
        assert_eq!(cookies.get("user"), Some(&"john".to_string()));
    }

    #[test]
    fn should_handle_cookie_with_no_value_no_equals() {
        let header = "session";
        let cookies = parse_cookies(header);

        assert!(cookies.is_empty());
    }

    #[test]
    fn should_handle_multiple_equals_in_value() {
        let header = "data=a=b=c=d";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("data"), Some(&"a=b=c=d".to_string()));
    }

    #[test]
    fn should_handle_cookie_name_with_whitespace_only() {
        let header = "   =value; user=john";
        let cookies = parse_cookies(header);

        assert!(cookies.get("").is_none());
        assert_eq!(cookies.get("user"), Some(&"john".to_string()));
    }

    #[test]
    fn should_handle_very_long_cookie_value() {
        let long_value = "a".repeat(4096);
        let header = format!("session={}", long_value);
        let cookies = parse_cookies(&header);

        assert_eq!(cookies.get("session"), Some(&long_value));
    }

    #[test]
    fn should_handle_special_characters_in_cookie_value() {
        let header = "data=hello%20world%2Btest; special=<>&\"'";
        let cookies = parse_cookies(header);

        assert_eq!(
            cookies.get("data"),
            Some(&"hello%20world%2Btest".to_string())
        );
        assert_eq!(cookies.get("special"), Some(&"<>&\"'".to_string()));
    }

    #[test]
    fn should_handle_unicode_in_cookie_value() {
        let header = "data=héllo wörld; emoji=🎉";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("data"), Some(&"héllo wörld".to_string()));
        assert_eq!(cookies.get("emoji"), Some(&"🎉".to_string()));
    }

    #[test]
    fn should_handle_multiple_semicolons() {
        let header = "a=1;;;b=2;;";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("a"), Some(&"1".to_string()));
        assert_eq!(cookies.get("b"), Some(&"2".to_string()));
        assert_eq!(cookies.len(), 2);
    }

    #[test]
    fn should_handle_leading_trailing_semicolons() {
        let header = "; session=abc; ; user=john; ";
        let cookies = parse_cookies(header);

        assert_eq!(cookies.get("session"), Some(&"abc".to_string()));
        assert_eq!(cookies.get("user"), Some(&"john".to_string()));
        assert_eq!(cookies.len(), 2);
    }

    #[test]
    fn should_handle_duplicate_cookie_names() {
        let header = "session=first; session=second";
        let cookies = parse_cookies(header);

        // Last value wins
        assert_eq!(cookies.get("session"), Some(&"second".to_string()));
    }

    #[test]
    fn should_parse_set_cookie_with_expires() {
        let header = "session=abc; Expires=Wed, 21 Oct 2025 07:28:00 GMT; Path=/";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("name"), Some(&"session".to_string()));
        assert_eq!(parsed.get("value"), Some(&"abc".to_string()));
        assert_eq!(
            parsed.get("expires"),
            Some(&"Wed, 21 Oct 2025 07:28:00 GMT".to_string())
        );
    }

    #[test]
    fn should_parse_set_cookie_case_insensitive_attributes() {
        let header = "session=abc; SECURE; HttpOnly; SAMesite=STrict; PATH=/api";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("secure"), Some(&"true".to_string()));
        assert_eq!(parsed.get("httponly"), Some(&"true".to_string()));
        assert_eq!(parsed.get("samesite"), Some(&"STrict".to_string()));
        assert_eq!(parsed.get("path"), Some(&"/api".to_string()));
    }

    #[test]
    fn should_parse_set_cookie_with_domain() {
        let header = "session=abc; Domain=.example.com; Path=/";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("domain"), Some(&".example.com".to_string()));
    }

    #[test]
    fn should_handle_empty_set_cookie() {
        let parsed = parse_set_cookie("");
        assert!(parsed.is_empty());
    }

    #[test]
    fn should_handle_set_cookie_without_attributes() {
        let header = "session=value";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("name"), Some(&"session".to_string()));
        assert_eq!(parsed.get("value"), Some(&"value".to_string()));
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn should_handle_set_cookie_with_only_boolean_attributes() {
        let header = "session=abc; Secure; HttpOnly";
        let parsed = parse_set_cookie(header);

        assert_eq!(parsed.get("secure"), Some(&"true".to_string()));
        assert_eq!(parsed.get("httponly"), Some(&"true".to_string()));
    }

    #[test]
    fn should_build_cookie_with_negative_max_age() {
        let cookie = CookieBuilder::new("session", "abc").max_age(-1).build();
        assert_eq!(cookie, "session=abc; Max-Age=-1");
    }

    #[test]
    fn should_build_cookie_with_zero_max_age() {
        let cookie = CookieBuilder::new("session", "abc").max_age(0).build();
        assert_eq!(cookie, "session=abc; Max-Age=0");
    }

    #[test]
    fn should_build_cookie_with_empty_value() {
        let cookie = CookieBuilder::new("session", "").build();
        assert_eq!(cookie, "session=");
    }

    #[test]
    fn should_build_cookie_with_special_characters_in_value() {
        let cookie = CookieBuilder::new("data", "hello world+test=foo").build();
        assert_eq!(cookie, "data=hello world+test=foo");
    }

    #[test]
    fn should_build_cookie_with_unicode_value() {
        let cookie = CookieBuilder::new("data", "héllo").build();
        assert_eq!(cookie, "data=héllo");
    }

    #[test]
    fn should_build_cookie_with_root_path() {
        let cookie = CookieBuilder::new("session", "abc").path("/").build();
        assert_eq!(cookie, "session=abc; Path=/");
    }

    #[test]
    fn should_build_cookie_with_subpath() {
        let cookie = CookieBuilder::new("session", "abc").path("/api/v1").build();
        assert_eq!(cookie, "session=abc; Path=/api/v1");
    }

    #[test]
    fn should_build_cookie_with_subdomain() {
        let cookie = CookieBuilder::new("session", "abc")
            .domain("api.example.com")
            .build();
        assert_eq!(cookie, "session=abc; Domain=api.example.com");
    }

    #[test]
    fn should_build_cookie_with_wildcard_domain() {
        let cookie = CookieBuilder::new("session", "abc")
            .domain(".example.com")
            .build();
        assert_eq!(cookie, "session=abc; Domain=.example.com");
    }

    #[test]
    fn should_create_delete_cookie_without_path() {
        let cookie = delete_cookie("session", None, None);
        assert!(cookie.contains("session="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(!cookie.contains("Path="));
        assert!(!cookie.contains("Domain="));
    }

    #[test]
    fn should_delete_cookie_parses_correctly() {
        let cookie = delete_cookie("my_session", Some("/api"), Some("example.com"));
        let parsed = parse_set_cookie(&cookie);

        assert_eq!(parsed.get("name"), Some(&"my_session".to_string()));
        assert_eq!(parsed.get("value"), Some(&"".to_string()));
        assert_eq!(parsed.get("max-age"), Some(&"0".to_string()));
        assert_eq!(parsed.get("path"), Some(&"/api".to_string()));
        assert_eq!(parsed.get("domain"), Some(&"example.com".to_string()));
    }
}
