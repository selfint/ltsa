use std::{
    error::Error,
    fmt::{Debug, Display},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Request<Params> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
    pub id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification<Params> {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Params>,
}

#[derive(Serialize, Deserialize, Debug)]
#[must_use = "this `Response`'s result may be a `JsonRpcError` variant, which should be handled"]
pub struct Response<T, E> {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub result: JsonRpcResult<T, E>,
    pub id: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
#[must_use]
pub enum JsonRpcResult<T, E> {
    Result(T),
    Error(JsonRpcError<E>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonRpcError<E> {
    pub code: i64,
    pub message: String,
    pub data: Option<E>,
}

impl<E: Debug> Error for JsonRpcError<E> {}

impl<E: Debug> Display for JsonRpcError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "[JsonRpcError] code: {} message: {}",
            self.code, self.message
        ))?;

        if let Some(data) = &self.data {
            f.write_fmt(format_args!("data: {:?}", data))?;
        };

        Ok(())
    }
}

impl<T, E: Debug> From<JsonRpcResult<T, E>> for Result<T, JsonRpcError<E>> {
    fn from(value: JsonRpcResult<T, E>) -> Self {
        match value {
            JsonRpcResult::Result(r) => Ok(r),
            JsonRpcResult::Error(e) => Err(e),
        }
    }
}

impl<T, E: Debug> JsonRpcResult<T, E> {
    pub fn as_result(self) -> Result<T, JsonRpcError<E>> {
        self.into()
    }
}
