use anyhow::{anyhow, Result};
use jsonrpc::{
    client::Client as JsonRpcClient,
    types::{JsonRpcError, Notification},
};
use lsp_types::{
    notification::Notification as LspNotification, request::Request as LspRequest,
    PublishDiagnosticsParams,
};
use serde_json::Value;
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

pub struct Client {
    jsonrpc_client: JsonRpcClient,
    encoder_handle: JoinHandle<()>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.encoder_handle.abort();
    }
}

impl Client {
    pub fn new(client_tx: UnboundedSender<String>, server_rx: UnboundedReceiver<String>) -> Self {
        let (jsonrpc_client_tx, jsonrpc_client_rx) = unbounded_channel();

        Self {
            jsonrpc_client: JsonRpcClient::with_handler(
                jsonrpc_client_tx,
                server_rx,
                |notification| -> Result<()> {
                    //
                    let diagnostics: Notification<PublishDiagnosticsParams> =
                        serde_json::from_value(notification)?;
                    let diagnostics = diagnostics
                        .params
                        .ok_or_else(|| anyhow!("got diagnostics without params"))?;

                    eprintln!(
                        "Got diagnostics from server:\npath: {}\ndiagnostics: {:#?}",
                        diagnostics.uri,
                        diagnostics
                            .diagnostics
                            .iter()
                            .map(|d| { d.message.to_string() })
                            .collect::<Vec<_>>()
                    );

                    Ok(())
                },
            ),
            encoder_handle: tokio::spawn(Client::lsp_encode(jsonrpc_client_rx, client_tx)),
        }
    }

    async fn lsp_encode(mut rx: UnboundedReceiver<String>, tx: UnboundedSender<String>) {
        while let Some(msg) = rx.recv().await {
            let len = msg.as_bytes().len();
            let msg = format!("Content-Length: {}\r\n\r\n{}", len, msg);
            tx.send(msg).expect("failed to send message");
        }
    }

    pub async fn request<R>(
        &self,
        params: R::Params,
    ) -> Result<Result<R::Result, JsonRpcError<Value>>>
    where
        R: LspRequest,
    {
        Ok(self
            .jsonrpc_client
            .request(R::METHOD.to_string(), Some(params))
            .await?
            .result
            .as_result())
    }

    pub fn notify<R>(&self, params: R::Params) -> Result<()>
    where
        R: LspNotification,
    {
        self.jsonrpc_client
            .notify(R::METHOD.to_string(), Some(params))
    }
}
