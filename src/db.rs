use crate::config::Config;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

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

pub async fn get_role(
    pool: &SqlitePool,
    chat_id: i64,
    user_id: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT role FROM admins WHERE chat_id = ? AND user_id = ?")
        .bind(chat_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn add_admin(pool: &SqlitePool, chat_id: i64, user_id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO admins (chat_id, user_id, role) VALUES (?, ?, 'admin') ON CONFLICT DO NOTHING",
    )
    .bind(chat_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn remove_admin(
    pool: &SqlitePool,
    chat_id: i64,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("DELETE FROM admins WHERE chat_id = ? AND user_id = ? AND role = 'admin'")
            .bind(chat_id)
            .bind(user_id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Clock is before 1970")
        .as_secs() as i64
}

pub async fn get_limits(
    pool: &SqlitePool,
    chat_id: i64,
) -> Result<Option<(i64, i64)>, sqlx::Error> {
    sqlx::query_as("SELECT action_limit, action_period_hours FROM chat_settings WHERE chat_id = ?")
        .bind(chat_id)
        .fetch_optional(pool)
        .await
}

pub async fn count_bans_since(
    pool: &SqlitePool,
    chat_id: i64,
    admin_id: i64,
    since: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM banned_users WHERE chat_id = ? AND banned_by = ? AND banned_at > ?",
    )
    .bind(chat_id)
    .bind(admin_id)
    .bind(since)
    .fetch_one(pool)
    .await
}

pub async fn ban_user(
    pool: &SqlitePool,
    chat_id: i64,
    user_id: i64,
    banned_by: i64,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO banned_users (chat_id, user_id, reason, status, banned_by, banned_at) \
         VALUES (?, ?, ?, 'banned', ?, ?)",
    )
    .bind(chat_id)
    .bind(user_id)
    .bind(reason)
    .bind(banned_by)
    .bind(now())
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE votes SET status = 'banned' WHERE chat_id = ? AND target_user_id = ? AND status = 'active'",
    )
    .bind(chat_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unban_user(
    pool: &SqlitePool,
    chat_id: i64,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM banned_users WHERE chat_id = ? AND user_id = ?")
        .bind(chat_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
