use crate::{sync_lots::sync_trade_update, update_server::UpdateNotification};
use apca::ApiInfo;
use async_trait::async_trait;
use serde_json::json;

struct SocketClient {}

impl SocketClient {
    fn process_json_message(text_msg: &str) {
        tracing::info!("websocket recv: {}", text_msg);
        let resp: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&text_msg);
        match resp {
            Ok(content) => {
                if content["data"].get("event").is_some() {
                    let res = sync_trade_update(&text_msg);
                    if let Err(e) = res {
                        tracing::error!("error syncing trade update: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("could not deserialize message: {}", e);
            }
        }
    }
}

#[async_trait]
impl ezsockets::ClientExt for SocketClient {
    type Params = ();

    async fn text(&mut self, text: String) -> Result<(), ezsockets::Error> {
        tracing::info!("AlpacaSocketClient recv: {text}");
        SocketClient::process_json_message(&text);
        Ok(())
    }

    async fn binary(&mut self, bytes: Vec<u8>) -> Result<(), ezsockets::Error> {
        tracing::info!("received {:?} bytes", bytes.len());
        let text = String::from_utf8_lossy(&bytes);
        SocketClient::process_json_message(&text);
        Ok(())
    }

    async fn call(&mut self, params: Self::Params) -> Result<(), ezsockets::Error> {
        tracing::info!("received params: {params:?}");
        let () = params;
        Ok(())
    }
}

pub async fn listen_for_trade_updates(tx: tokio::sync::mpsc::Sender<UpdateNotification>) -> Result<(), tungstenite::Error> {
    let api_info = ApiInfo::from_env().unwrap();

    let mut url = api_info.data_stream_base_url;
    url.set_path("/stream");

    let config = ezsockets::ClientConfig::new(url);

    let (client, future) = ezsockets::connect(|_client| SocketClient {}, config).await;
    tokio::spawn(async move {
        future.await.unwrap();
    });

    let auth =
        json!({ "action": "auth", "key": api_info.key_id, "secret": api_info.secret }).to_string();
    client.text(auth);

    client.text(r#"{ "action": "listen", "data": { "streams": ["trade_updates"] } }"#.into());

    Ok(())
}
