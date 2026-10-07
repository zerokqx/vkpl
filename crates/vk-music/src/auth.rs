use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default = "root")]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub expires: Option<f64>,
}
fn root() -> String {
    "/".into()
}

impl Cookie {
    pub fn is_vk(&self) -> bool {
        let domain = self.domain.trim_start_matches('.');
        ["vk.ru", "vk.com"]
            .iter()
            .any(|base| domain == *base || domain.ends_with(&format!(".{base}")))
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if !self.is_vk()
            || self.name.is_empty()
            || self.name.contains([';', '=', '\r', '\n', ' '])
            || self.value.contains([';', '\r', '\n'])
            || !self.path.starts_with('/')
            || self.path.contains([';', '\r', '\n'])
        {
            return Err(Error::InvalidInput("некорректная cookie VK".into()));
        }
        Ok(())
    }
    pub(crate) fn alive(&self) -> bool {
        self.expires.is_none_or(|v| v <= 0.0 || v > now() as f64)
    }
}

/// Credentials have a redacted Debug implementation. Serialize only to private storage.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub(crate) access_token: String,
    #[serde(default)]
    pub(crate) expires_at: u64,
    #[serde(default)]
    pub(crate) cookies: Vec<Cookie>,
    #[serde(default)]
    pub user_id: Option<i64>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id)
            .field("refreshable", &self.can_refresh())
            .finish_non_exhaustive()
    }
}

impl Session {
    pub fn token(token: impl Into<String>) -> Result<Self> {
        let token = token.into().trim().to_owned();
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return Err(Error::InvalidInput(
                "пустой токен или пробелы в токене".into(),
            ));
        }
        Ok(Self {
            access_token: token,
            expires_at: 0,
            cookies: vec![],
            user_id: None,
        })
    }
    pub fn cookies(cookies: Vec<Cookie>) -> Result<Self> {
        let cookies: Vec<_> = cookies
            .into_iter()
            .filter(|c| c.is_vk() && c.alive())
            .collect();
        for cookie in &cookies {
            cookie.validate()?;
        }
        if !cookies
            .iter()
            .any(|c| c.name == "remixsid" && !c.value.is_empty())
        {
            return Err(Error::InvalidInput(
                "нет действующей remixsid cookie VK".into(),
            ));
        }
        Ok(Self {
            access_token: String::new(),
            expires_at: 0,
            cookies,
            user_id: None,
        })
    }
    /// Accepts a browser JSON cookie export or a Netscape cookies.txt file.
    pub fn import(text: &str) -> Result<Self> {
        if text.trim_start().starts_with('[') {
            let cookies = serde_json::from_str(text)
                .map_err(|_| Error::InvalidInput("ожидался JSON-массив cookies".into()))?;
            return Self::cookies(cookies);
        }
        let mut cookies = Vec::new();
        for line in text.lines() {
            let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<_> = line.splitn(7, '\t').collect();
            if parts.len() != 7 {
                return Err(Error::InvalidInput("ожидался Netscape cookies.txt".into()));
            }
            cookies.push(Cookie {
                domain: parts[0].into(),
                path: parts[2].into(),
                secure: parts[3] == "TRUE",
                expires: Some(
                    parts[4]
                        .parse()
                        .map_err(|_| Error::InvalidInput("срок cookie".into()))?,
                ),
                name: parts[5].into(),
                value: parts[6].into(),
            });
        }
        Self::cookies(cookies)
    }
    pub fn can_refresh(&self) -> bool {
        !self.cookies.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netscape_http_only_and_domain_isolation() {
        let session = Session::import("# Netscape HTTP Cookie File\n#HttpOnly_.vk.ru\tTRUE\t/\tTRUE\t0\tremixsid\tsession-secret\n.evilvk.ru\tTRUE\t/\tTRUE\t0\tremixsid\tbad\n.example.org\tTRUE\t/\tTRUE\t0\tother\tsecret\n").unwrap();
        assert_eq!(session.cookies.len(), 1);
        assert_eq!(session.cookies[0].value, "session-secret");
        assert!(!format!("{session:?}").contains("session-secret"));
    }

    #[test]
    fn json_export_accepts_cdp_session_cookies() {
        let session = Session::import(r#"[{"name":"remixsid","value":"secret","domain":".vk.ru","path":"/","secure":true,"expires":-1,"httpOnly":true}]"#).unwrap();
        assert!(session.can_refresh());
    }

    #[test]
    fn rejects_expired_missing_and_injected_cookies() {
        assert!(Session::import(".vk.ru\tTRUE\t/\tTRUE\t1\tremixsid\tsecret").is_err());
        assert!(Session::import(".example.org\tTRUE\t/\tTRUE\t0\tremixsid\tsecret").is_err());
        assert!(Session::import(".vk.ru\tTRUE\t/\tTRUE\t0\tremixsid\tx; Secure").is_err());
        assert!(Session::token("token\nheader").is_err());
    }
}
