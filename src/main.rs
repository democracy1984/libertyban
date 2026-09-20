mod commands;
mod config;
mod db;

use teloxide::prelude::*;

#[tokio::main]
async fn main() {
    let config = config::load();
    let pool = db::init(&config).await;
    let bot = Bot::new(config.telegram_api_key.clone());
    println!("Bot is ready");

    commands::run(bot, pool).await;
}
