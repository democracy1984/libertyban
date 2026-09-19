CREATE TABLE admins (
    chat_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner','admin')),
    PRIMARY KEY (chat_id, user_id)
);
CREATE UNIQUE INDEX one_owner_per_chat ON admins(chat_id) WHERE role = 'owner';

CREATE TABLE banned_users (
    chat_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    reason TEXT,
    status Text NOT NULL DEFAULT 'banned' CHECK (status IN ('banned', 'muted')),
    banned_by INTEGER,
    banned_at INTEGER NOT NULL,
    PRIMARY KEY (chat_id, user_id)
);

CREATE TABLE chat_settings(
    chat_id INTEGER PRIMARY KEY,
    required_votes INTEGER NOT NULL DEFAULT 5 CHECK (required_votes >= 2),
    action_limit INTEGER NOT NULL DEFAULT 5,
    action_period_hours INTEGER NOT NULL DEFAULT 6
);


CREATE TABLE votes (
  id              INTEGER PRIMARY KEY,
  chat_id         INTEGER NOT NULL,
  target_user_id  INTEGER NOT NULL,
  starter_user_id INTEGER NOT NULL,
  message_id      INTEGER,
  reasons         TEXT,
  status          TEXT NOT NULL DEFAULT 'active'
                  CHECK (status IN ('active','banned','cancelled','expired')),
  created_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX one_active_vote
  ON votes(chat_id, target_user_id) WHERE status = 'active';

CREATE TABLE vote_users (
  vote_id    INTEGER NOT NULL REFERENCES votes(id),
  user_id    INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (vote_id, user_id)
);
