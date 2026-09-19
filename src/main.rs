mod config;
mod db;

use teloxide::{prelude::*, types::ChatId};

#[tokio::main]
async fn main() {
    let config = config::load();
    let _pool = db::init(&config).await;

    let bot = Bot::new(config.telegram_api_key.clone());
    bot.send_message(ChatId(config.owner_chat), "db is ready")
        .await
        .expect("Failed to send message");
}
