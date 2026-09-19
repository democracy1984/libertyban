use std::env;

pub struct Config {
    pub telegram_api_key: String,
    pub db_url: String,
    pub owner_id: i64,
    pub owner_chat: i64,
    pub required_votes: i64,
    pub action_limit: i64,
    pub action_period_hours: i64,
}

fn env_str(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

fn env_i64(name: &str) -> i64 {
    env_str(name)
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be a number"))
}

pub fn load() -> Config {
    dotenvy::dotenv().expect("Failed to load .env");

    Config {
        telegram_api_key: env_str("TELEGRAM_API_KEY"),
        db_url: env_str("SQLITE_DB_URL"),
        owner_id: env_i64("OWNER_ID"),
        owner_chat: env_i64("OWNER_CHAT"),
        required_votes: env_i64("REQUIRED_VOTES"),
        action_limit: env_i64("ACTION_LIMIT"),
        action_period_hours: env_i64("ACTION_PERIOD_HOURS"),
    }
}
