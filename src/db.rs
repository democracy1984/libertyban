use crate::config::Config;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;

pub async fn init(config: &Config) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(&config.db_url)
        .expect("Bad SQLITE_DB_URL")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .expect("Failed to open database");

    let admins_table: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'admins'",
    )
    .fetch_optional(&pool)
    .await
    .expect("Failed to check database");

    if admins_table.is_some() {
        println!("Database already exists, not touching it");
    } else {
        create_new(&pool, config).await;
        println!("Database created");
    }
    pool
}

async fn create_new(pool: &SqlitePool, config: &Config) {
    sqlx::raw_sql(include_str!("../migrations/0001_init.sql"))
        .execute(pool)
        .await
        .expect("Failed to create tables");

    sqlx::query("INSERT INTO admins (chat_id, user_id, role) VALUES (?, ?, 'owner')")
        .bind(config.owner_chat)
        .bind(config.owner_id)
        .execute(pool)
        .await
        .expect("Failed to add owner");

    sqlx::query(
        "INSERT INTO chat_settings (chat_id, required_votes, action_limit, action_period_hours) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(config.owner_chat)
    .bind(config.required_votes)
    .bind(config.action_limit)
    .bind(config.action_period_hours)
    .execute(pool)
    .await
    .expect("Failed to add chat settings");
}
