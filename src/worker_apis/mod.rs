use std::error;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ResponseContent<T> {
    pub status: u16,
    pub content: String,
    pub entity: Option<T>,
}

#[derive(Debug)]
pub enum Error<T> {
    Js(wasm_bindgen::JsValue),
    Serde(serde_json::Error),
    Io(std::io::Error),
    Worker(worker::Error),
    ResponseError(ResponseContent<T>),
}

impl<T> fmt::Display for Error<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (module, e) = match self {
            Error::Js(e) => ("wasm_bindgen", format!("{:?}", e)),
            Error::Serde(e) => ("serde", e.to_string()),
            Error::Io(e) => ("IO", e.to_string()),
            Error::Worker(e) => ("Worker", e.to_string()),
            Error::ResponseError(e) => ("response", format!("status code {}", e.status)),
        };
        write!(f, "error in {}: {}", module, e)
    }
}

impl<T: fmt::Debug> error::Error for Error<T> {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(match self {
            Error::Js(_) => return None,
            Error::Serde(e) => e,
            Error::Io(e) => e,
            Error::Worker(e) => e,
            Error::ResponseError(_) => return None,
        })
    }
}

impl<T> From<wasm_bindgen::JsValue> for Error<T> {
    fn from(e: wasm_bindgen::JsValue) -> Self {
        Error::Js(e)
    }
}

impl<T> From<serde_json::Error> for Error<T> {
    fn from(e: serde_json::Error) -> Self {
        Error::Serde(e)
    }
}

impl<T> From<std::io::Error> for Error<T> {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl<T> From<worker::Error> for Error<T> {
    fn from(e: worker::Error) -> Self {
        Error::Worker(e)
    }
}

trait AddQuery {
    fn add_query(&mut self, first_query: &mut bool, param: &str, value: &str);
}

impl AddQuery for String {
    fn add_query(&mut self, first_query: &mut bool, param: &str, value: &str) {
        if *first_query {
            self.push('?');
            *first_query = false;
        } else {
            self.push('&');
        }
        self.push_str(param);
        self.push_str(value);
    }
}

pub fn urlencode<T: AsRef<str>>(s: T) -> String {
    ::url::form_urlencoded::byte_serialize(s.as_ref().as_bytes()).collect()
}

pub fn parse_deep_object(prefix: &str, value: &serde_json::Value) -> Vec<(String, String)> {
    if let serde_json::Value::Object(object) = value {
        let mut params: Vec<(String, String)> = vec![];

        for (key, value) in object {
            match value {
                serde_json::Value::Object(_) => params.append(&mut parse_deep_object(
                    &format!("{}[{}]", prefix, key),
                    value,
                )),
                serde_json::Value::Array(array) => {
                    for (i, value) in array.iter().enumerate() {
                        params.append(&mut parse_deep_object(
                            &format!("{}[{}][{}]", prefix, key, i),
                            value,
                        ));
                    }
                }
                serde_json::Value::String(s) => {
                    params.push((format!("{}[{}]", prefix, key), s.clone()))
                }
                _ => params.push((format!("{}[{}]", prefix, key), value.to_string())),
            }
        }

        return params;
    }

    unimplemented!("Only objects are supported with style=deepObject")
}

/// Internal use only
/// A content type supported by this client.
enum ContentType {
    Json,
    Text,
    Unsupported(String),
    Missing,
}

impl From<&str> for ContentType {
    fn from(content_type: &str) -> Self {
        if content_type.starts_with("application") && content_type.contains("json") {
            Self::Json
        } else if content_type.starts_with("text/plain") {
            Self::Text
        } else {
            Self::Unsupported(content_type.to_string())
        }
    }
}

impl From<&worker::Response> for ContentType {
    fn from(resp: &worker::Response) -> Self {
        let content_type = resp.headers().get("content-type").unwrap_or_default();

        let Some(content_type) = content_type else {
            return Self::Missing;
        };

        Self::from(content_type.as_str())
    }
}

pub mod configuration;
pub mod courier_api;
pub mod frontend_api;
pub mod identity_api;
pub mod metadata_api;
