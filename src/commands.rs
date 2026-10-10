use crate::db;
use sqlx::SqlitePool;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode, User};
use teloxide::utils::html::escape;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Help,
    GiveAdmin,
    RemoveAdmin,
    Ban,
    Unban(i64),
    Report,
}

const OWNER_HELP: &str =
    "Команды владельца:\n/help\n/giveadmin\n/removeadmin\n/ban\n/unban\n/report";
const ADMIN_HELP: &str = "Команды администратора:\n/help\n/ban\n/unban\n/report";
const USER_HELP: &str = "Команды:\n/help\n/report";

pub async fn run(bot: Bot, pool: SqlitePool) {
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handle),
        )
        .branch(Update::filter_callback_query().endpoint(support));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![pool])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn handle(bot: Bot, msg: Message, cmd: Command, pool: SqlitePool) -> ResponseResult<()> {
    match cmd {
        Command::Help => help(&bot, &msg, &pool).await,
        Command::GiveAdmin => give_admin(&bot, &msg, &pool).await,
        Command::RemoveAdmin => remove_admin(&bot, &msg, &pool).await,
        Command::Ban => ban(&bot, &msg, &pool).await,
        Command::Unban(user_id) => unban(&bot, &msg, &pool, user_id).await,
        Command::Report => report(&bot, &msg, &pool).await,
    }
}

fn mention(user: &User) -> String {
    format!(
        r#"<a href="tg://user?id={}">{}</a>"#,
        user.id.0,
        escape(&user.full_name())
    )
}

async fn role_of(pool: &SqlitePool, chat: ChatId, user: UserId) -> Option<String> {
    match db::get_role(pool, chat.0, user.0 as i64).await {
        Ok(role) => role,
        Err(e) => {
            log::error!("DB error: {e}");
            None
        }
    }
}

fn has_role(role: Option<&str>, allowed: &[&str]) -> bool {
    role.is_some_and(|r| allowed.contains(&r))
}

fn reason_of(msg: &Message) -> Option<String> {
    let text = msg.text()?;
    let (_, rest) = text.split_once(char::is_whitespace)?;
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

async fn help(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    log::debug!("/help from user {} in chat {}", user.id.0, msg.chat.id.0);

    let text = match role_of(pool, msg.chat.id, user.id).await.as_deref() {
        Some("owner") => OWNER_HELP,
        Some("admin") => ADMIN_HELP,
        _ => USER_HELP,
    };
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn give_admin(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    if role_of(pool, msg.chat.id, user.id).await.as_deref() != Some("owner") {
        log::warn!(
            "Permission denied: chat={} by={} command=giveadmin",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Эта команда только для владельца")
            .await?;
        return Ok(());
    }

    let Some(target) = msg.reply_to_message().and_then(|m| m.from.as_ref()) else {
        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение человека")
            .await?;
        return Ok(());
    };

    if target.is_bot {
        bot.send_message(msg.chat.id, "Боту права выдать нельзя")
            .await?;
        return Ok(());
    }

    let chat = msg.chat.id.0;
    let by = user.id.0;
    let target_id = target.id.0;

    let text = match db::add_admin(pool, chat, target_id as i64).await {
        Ok(true) => {
            log::info!("give_admin chat={chat} by={by} target={target_id}");
            format!("{} теперь администратор", mention(target))
        }
        Ok(false) => "Этот человек уже админ или владелец".to_string(),
        Err(e) => {
            log::error!("give_admin chat={chat} by={by} target={target_id}: DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

async fn remove_admin(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    // Только владелец
    if role_of(pool, msg.chat.id, user.id).await.as_deref() != Some("owner") {
        log::warn!(
            "Permission denied: chat={} by={} command=removeadmin",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Эта команда только для владельца")
            .await?;
        return Ok(());
    }

    let Some(target) = msg.reply_to_message().and_then(|m| m.from.as_ref()) else {
        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение человека")
            .await?;
        return Ok(());
    };

    let chat = msg.chat.id.0;
    let by = user.id.0;
    let target_id = target.id.0;

    let text = match db::remove_admin(pool, chat, target_id as i64).await {
        Ok(true) => {
            log::info!("remove_admin chat={chat} by={by} target={target_id}");
            format!("{} больше не администратор", mention(target))
        }
        Ok(false) => "Он и не был админом".to_string(),
        Err(e) => {
            log::error!("remove_admin chat={chat} by={by} target={target_id}: DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

async fn ban(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    // Админ или владелец
    let role = role_of(pool, msg.chat.id, user.id).await;

    if !has_role(role.as_deref(), &["owner", "admin"]) {
        log::warn!(
            "Permission denied: chat={} by={} command=ban",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Эта команда только для админов и владельца")
            .await?;

        return Ok(());
    }

    let is_owner = role.as_deref() == Some("owner");

    let Some(reply) = msg.reply_to_message() else {
        log::warn!(
            "Ban denied: chat={} by={} reason=no_reply",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение нарушителя")
            .await?;
        return Ok(());
    };

    let Some(target) = &reply.from else {
        log::warn!(
            "Ban denied: chat={} by={} reason=no_target",
            msg.chat.id.0,
            user.id.0
        );

        return Ok(());
    };

    // Не бот и не сам юзер
    if target.is_bot {
        log::warn!(
            "Ban denied: chat={} by={} target={} reason=bot",
            msg.chat.id.0,
            user.id.0,
            target.id.0
        );

        bot.send_message(msg.chat.id, "Этого пользователя банить нельзя")
            .await?;
        return Ok(());
    }

    if target.id == user.id {
        log::warn!(
            "Ban denied: chat={} by={} reason=self",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Зачем тебе репортить самого себя?")
            .await?;
        return Ok(());
    }

    if role_of(pool, msg.chat.id, target.id).await.is_some() {
        log::warn!(
            "Ban denied: chat={} by={} target={} reason=target_is_admin",
            msg.chat.id.0,
            user.id.0,
            target.id.0
        );

        bot.send_message(msg.chat.id, "Админов и владельца банить нельзя")
            .await?;
        return Ok(());
    }

    // лимит для админов (у владельца ограничений нет)
    if !is_owner {
        let (limit, hours) = match db::get_limits(pool, msg.chat.id.0).await {
            Ok(Some(limits)) => limits,
            Ok(None) => (5, 6), // чата нет в настройках: значения по умолчанию
            Err(e) => {
                log::error!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        };
        let since = db::now() - hours * 3600;
        match db::count_bans_since(pool, msg.chat.id.0, user.id.0 as i64, since).await {
            Ok(n) if n >= limit => {
                log::info!(
                    "limit: chat={} by={} bans={}/{} hours={}",
                    msg.chat.id.0,
                    user.id.0,
                    n,
                    limit,
                    hours
                );
                bot.send_message(
                    msg.chat.id,
                    format!("Лимит: не больше {limit} банов за {hours} ч"),
                )
                .await?;
                return Ok(());
            }
            Ok(_) => {}
            Err(e) => {
                log::error!("DB error: {e}");
                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;
                return Ok(());
            }
        }
    }

    // бан в Telegram
    if let Err(e) = bot.ban_chat_member(msg.chat.id, target.id).await {
        log::error!("Ban failed: {e}");
        bot.send_message(
            msg.chat.id,
            "Не удалось забанить. Бот должен быть админом с правом блокировать участников",
        )
        .await?;
        return Ok(());
    }

    // запись в базу
    let reason = reason_of(msg);
    if let Err(e) = db::ban_user(
        pool,
        msg.chat.id.0,
        target.id.0 as i64,
        Some(user.id.0 as i64),
        reason.as_deref(),
    )
    .await
    {
        // в Telegram бан уже прошёл, но в базу не записался
        log::error!("DB error: {e}");
    }

    // удаляем сообщение нарушителя (если ему меньше 48 часов)
    if let Err(e) = bot.delete_message(msg.chat.id, reply.id).await {
        log::error!("Delete failed: {e}");
    }

    let text = match &reason {
        Some(r) => format!("{} забанен. Причина: {}", mention(target), escape(r)),
        None => format!("{} забанен", mention(target)),
    };
    log::info!(
        "User banned: chat={} by={} target={}",
        msg.chat.id.0,
        user.id.0,
        target.id.0
    );
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

async fn unban(bot: &Bot, msg: &Message, pool: &SqlitePool, user_id: i64) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    let role = role_of(pool, msg.chat.id, user.id).await;

    if !has_role(role.as_deref(), &["owner", "admin"]) {
        log::warn!(
            "Permission denied: chat={} by={} command=unban",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Эта команда только для админов и владельца")
            .await?;
        return Ok(());
    }

    // сначала снимаем ограничение в Telegram, чтобы человек мог зайти снова
    if let Err(e) = bot
        .unban_chat_member(msg.chat.id, UserId(user_id as u64))
        .only_if_banned(true)
        .await
    {
        log::error!("Unban failed: {e}");
        bot.send_message(msg.chat.id, "Не удалось разбанить в Telegram")
            .await?;
        return Ok(());
    }

    let text = match db::unban_user(pool, msg.chat.id.0, user_id).await {
        Ok(true) => {
            log::info!(
                "User unbanned: chat={} by={} target={}",
                msg.chat.id.0,
                user.id.0,
                user_id
            );

            "Пользователь разбанен".to_string()
        }

        Ok(false) => {
            log::info!(
                "Unban skipped: user is not banned: chat={} by={} target={}",
                msg.chat.id.0,
                user.id.0,
                user_id
            );

            "Этого пользователя и не было в бане".to_string()
        }

        Err(e) => {
            log::error!("DB error: {e}");
            "Внутренняя ошибка, попробуй позже".to_string()
        }
    };

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;
    Ok(())
}

async fn report(bot: &Bot, msg: &Message, pool: &SqlitePool) -> ResponseResult<()> {
    let Some(user) = &msg.from else {
        return Ok(());
    };

    let Some(reply) = msg.reply_to_message() else {
        log::warn!(
            "Report denied: chat={} by={} reason=no_reply",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Ответь этой командой на сообщение нарушителя")
            .await?;
        return Ok(());
    };

    let Some(target) = &reply.from else {
        log::warn!(
            "Report denied: chat={} by={} reason=no_target",
            msg.chat.id.0,
            user.id.0
        );

        return Ok(());
    };

    if target.is_bot {
        log::warn!(
            "Report denied: chat={} by={} target={} reason=bot",
            msg.chat.id.0,
            user.id.0,
            target.id.0
        );

        bot.send_message(msg.chat.id, "Этого пользователя нельзя репортить")
            .await?;
        return Ok(());
    }

    if target.id == user.id {
        log::warn!(
            "Report denied: chat={} by={} reason=self",
            msg.chat.id.0,
            user.id.0
        );

        bot.send_message(msg.chat.id, "Зачем тебе репортить самого себя?")
            .await?;
        return Ok(());
    }

    let target_role = role_of(pool, msg.chat.id, target.id).await;

    if has_role(target_role.as_deref(), &["owner", "admin"]) {
        log::warn!(
            "Report denied: chat={} by={} target={} reason=target_is_admin",
            msg.chat.id.0,
            user.id.0,
            target.id.0
        );

        bot.send_message(msg.chat.id, "Админов и владельца репортить нельзя")
            .await?;
        return Ok(());
    }

    // Админов чата в Telegram бот забанить не может — не начинаем голосование
    if let Ok(member) = bot.get_chat_member(msg.chat.id, target.id).await {
        if member.is_privileged() {
            log::warn!(
                "Report denied: chat={} by={} target={} reason=target_is_admin_telegram",
                msg.chat.id.0,
                user.id.0,
                target.id.0
            );

            bot.send_message(msg.chat.id, "Админов чата банить нельзя")
                .await?;
            return Ok(());
        }
    }

    let reason = reason_of(msg);

    // Сообщение от имени чата (анонимный админ): его голос не записываем
    let anonymous = msg.sender_chat.is_some();

    // Есть ли уже активное голосование против этого человека
    let existing = match db::active_vote(pool, msg.chat.id.0, target.id.0 as i64).await {
        Ok(vote) => vote,

        Err(e) => {
            log::error!("DB error: {e}");

            bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                .await?;

            return Ok(());
        }
    };

    let vote_id = if let Some(vote_id) = existing {
        if anonymous {
            log::info!(
                "Anonymous report: chat={} target={} vote_id={}",
                msg.chat.id.0,
                target.id.0,
                vote_id
            );
        } else {
            // Голосование уже идёт: добавляем свой голос
            match db::add_vote(pool, vote_id, user.id.0 as i64, reason.as_deref()).await {
                Ok(true) => {
                    log::info!(
                        "Vote added: chat={} by={} target={} vote_id={}",
                        msg.chat.id.0,
                        user.id.0,
                        target.id.0,
                        vote_id
                    );
                }

                Ok(false) => {
                    log::info!(
                        "Duplicate vote: chat={} by={} target={} vote_id={}",
                        msg.chat.id.0,
                        user.id.0,
                        target.id.0,
                        vote_id
                    );

                    bot.send_message(msg.chat.id, "Ты уже голосовал за этого человека")
                        .await?;

                    return Ok(());
                }

                Err(e) => {
                    log::error!("DB error: {e}");

                    bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                        .await?;

                    return Ok(());
                }
            }
        }

        vote_id
    } else {
        // Голосования нет: проверяем лимит.
        // Лимит действует только на создание новых голосований.
        let (limit, hours) = match db::get_limits(pool, msg.chat.id.0).await {
            Ok(Some(limits)) => limits,

            Ok(None) => (5, 6),

            Err(e) => {
                log::error!("DB error: {e}");

                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;

                return Ok(());
            }
        };

        let since = db::now() - hours * 3600;

        match db::count_reports_since(pool, msg.chat.id.0, user.id.0 as i64, since).await {
            Ok(n) if n >= limit => {
                log::info!(
                    "Report limit reached: chat={} by={} reports={}/{} hours={}",
                    msg.chat.id.0,
                    user.id.0,
                    n,
                    limit,
                    hours
                );

                bot.send_message(
                    msg.chat.id,
                    format!(
                        "Репортить больше не можешь: лимит {limit} репортов за {hours} ч. \
                         Это мера против врагов демократии"
                    ),
                )
                .await?;

                return Ok(());
            }

            Ok(_) => {}

            Err(e) => {
                log::error!("DB error: {e}");

                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;

                return Ok(());
            }
        }

        // Создаём голосование.
        // Автор репорта становится первым голосом, анонимный — нет.
        match db::start_vote(
            pool,
            msg.chat.id.0,
            target.id.0 as i64,
            user.id.0 as i64,
            reply.id.0 as i64,
            reason.as_deref(),
            !anonymous,
        )
        .await
        {
            Ok(id) => {
                log::info!(
                    "Vote started: chat={} by={} target={} vote_id={}",
                    msg.chat.id.0,
                    user.id.0,
                    target.id.0,
                    id
                );

                id
            }

            Err(e) => {
                log::error!("DB error: {e}");

                bot.send_message(msg.chat.id, "Внутренняя ошибка, попробуй позже")
                    .await?;

                return Ok(());
            }
        }
    };

    // Считаем голоса и либо обновляем сообщение, либо баним при кворуме.
    apply_vote(bot, pool, msg.chat.id, vote_id, target, Some(reply.id)).await
}

// Считает голоса: обновляет сообщение голосования или банит при кворуме.
async fn apply_vote(
    bot: &Bot,
    pool: &SqlitePool,
    chat: ChatId,
    vote_id: i64,
    target: &User,
    reported_id: Option<MessageId>,
) -> ResponseResult<()> {
    let (Ok(count), Ok(required)) = (
        db::vote_count(pool, vote_id).await,
        db::required_votes(pool, chat.0).await,
    ) else {
        log::error!(
            "Failed to get vote count/required votes: chat={} vote_id={}",
            chat.0,
            vote_id
        );

        bot.send_message(chat, "Внутренняя ошибка, попробуй позже")
            .await?;

        return Ok(());
    };

    // Кворум не набран: пересылаем сообщение голосования с новым счётом
    if count < required {
        return send_vote_message(bot, pool, chat, vote_id, target, count, required).await;
    }

    log::info!(
        "Vote quorum reached: chat={} target={} vote_id={} votes={}/{}",
        chat.0,
        target.id.0,
        vote_id,
        count,
        required
    );

    // Бан в Telegram
    if let Err(e) = bot.ban_chat_member(chat, target.id).await {
        log::error!("Ban failed: {e}");

        // Отменяем голосование, чтобы кнопка не пыталась банить снова
        if let Err(e) = db::cancel_vote(pool, vote_id).await {
            log::error!("DB error: {e}");
        }

        if let Ok(Some(old_id)) = db::vote_message(pool, vote_id).await {
            if let Err(e) = bot.delete_message(chat, MessageId(old_id as i32)).await {
                log::warn!("Failed to delete vote message: {e}");
            }
        }

        bot.send_message(
            chat,
            "Голосование набрало кворум, но бан не удался. Голосование отменено. \
             Бот должен быть админом с правом блокировать участников",
        )
        .await?;

        return Ok(());
    }

    log::info!(
        "User banned by vote: chat={} target={} vote_id={} votes={}/{}",
        chat.0,
        target.id.0,
        vote_id,
        count,
        required
    );

    // Запись в базу
    if let Err(e) = db::ban_user(pool, chat.0, target.id.0 as i64, None, None).await {
        log::error!("DB error while saving ban: {e}");
    }

    // Удаляем сообщение с голосованием
    if let Ok(Some(old_id)) = db::vote_message(pool, vote_id).await {
        if let Err(e) = bot.delete_message(chat, MessageId(old_id as i32)).await {
            log::warn!("Failed to delete vote message: {e}");
        }
    }

    // Удаляем сообщение нарушителя
    if let Some(id) = reported_id {
        if let Err(e) = bot.delete_message(chat, id).await {
            log::warn!("Delete failed: {e}");
        }
    }

    let reasons = db::vote_reasons(pool, vote_id).await.unwrap_or_default();

    let mut text = format!(
        "{} был совершенно справедливо забанен демократическим голосованием ({count}/{required})",
        mention(target)
    );

    if !reasons.is_empty() {
        text.push_str("\nПричины:");

        for reason in &reasons {
            text.push_str(&format!("\n• {}", escape(reason)));
        }
    }

    bot.send_message(chat, text)
        .parse_mode(ParseMode::Html)
        .await?;

    Ok(())
}

async fn send_vote_message(
    bot: &Bot,
    pool: &SqlitePool,
    chat: ChatId,
    vote_id: i64,
    target: &User,
    count: i64,
    required: i64,
) -> ResponseResult<()> {
    // Удаляем старое сообщение голосования
    if let Ok(Some(old_id)) = db::vote_message(pool, vote_id).await {
        if let Err(e) = bot.delete_message(chat, MessageId(old_id as i32)).await {
            log::warn!(
                "Failed to delete old vote message: chat={} vote_id={} message_id={} error={}",
                chat.0,
                vote_id,
                old_id,
                e
            );
        }
    }

    let reasons = db::vote_reasons(pool, vote_id).await.unwrap_or_default();

    let mut text = format!(
        "Начато демократическое голосование за кик {}\nГолосов: {count}/{required}",
        mention(target)
    );

    if !reasons.is_empty() {
        text.push_str("\nПричины:");

        for reason in &reasons {
            text.push_str(&format!("\n• {}", escape(reason)));
        }
    }

    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "Поддержать",
        format!("vote:{vote_id}"),
    )]]);

    let sent = bot
        .send_message(chat, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    log::info!(
        "Vote message sent: chat={} vote_id={} target={} message_id={} votes={}/{}",
        chat.0,
        vote_id,
        target.id.0,
        sent.id.0,
        count,
        required
    );

    if let Err(e) = db::set_vote_message(pool, vote_id, sent.id.0 as i64).await {
        log::error!(
            "Failed to save vote message: chat={} vote_id={} message_id={} error={}",
            chat.0,
            vote_id,
            sent.id.0,
            e
        );
    }

    Ok(())
}

// Кнопка "Поддержать": добавляем голос и запускаем общую проверку кворума.
async fn support(bot: Bot, q: CallbackQuery, pool: SqlitePool) -> ResponseResult<()> {
    let Some(vote_id) = q
        .data
        .as_deref()
        .and_then(|d| d.strip_prefix("vote:")?.parse().ok())
    else {
        return Ok(());
    };

    let Some((chat, target, _, reported_id)) = db::vote_info(&pool, vote_id).await.ok().flatten()
    else {
        bot.answer_callback_query(q.id)
            .text("Голосование уже завершено")
            .await?;

        return Ok(());
    };

    match db::add_vote(&pool, vote_id, q.from.id.0 as i64, None).await {
        Ok(true) => {
            log::info!(
                "Vote added: chat={} by={} target={} vote_id={}",
                chat,
                q.from.id.0,
                target,
                vote_id
            );
        }

        Ok(false) => {
            log::info!(
                "Duplicate vote: chat={} by={} target={} vote_id={}",
                chat,
                q.from.id.0,
                target,
                vote_id
            );

            bot.answer_callback_query(q.id)
                .text("Ты уже голосовал за этого человека")
                .await?;

            return Ok(());
        }

        Err(e) => {
            log::error!("DB error: {e}");

            bot.answer_callback_query(q.id)
                .text("Внутренняя ошибка, попробуй позже")
                .await?;

            return Ok(());
        }
    }

    bot.answer_callback_query(q.id).text("Голос учтён").await?;

    // Имя цели нужно для текста сообщения голосования
    let Ok(member) = bot
        .get_chat_member(ChatId(chat), UserId(target as u64))
        .await
    else {
        return Ok(());
    };

    apply_vote(
        &bot,
        &pool,
        ChatId(chat),
        vote_id,
        &member.user,
        reported_id.map(|id| MessageId(id as i32)),
    )
    .await
}
