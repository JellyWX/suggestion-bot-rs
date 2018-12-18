CREATE TABLE suggestion.servers (
    map_id INT UNSIGNED AUTO_INCREMENT,
    id BIGINT UNSIGNED UNIQUE NOT NULL,
    prefix VARCHAR(5) DEFAULT "~" NOT NULL,
    role BIGINT UNSIGNED,
    threshold TINYINT UNSIGNED DEFAULT 10 NOT NULL,
    suggest_channel BIGINT UNSIGNED,
    approve_channel BIGINT UNSIGNED,
    bans JSON NOT NULL,
    upvote_emoji VARCHAR(255) DEFAULT "✅" NOT NULL,
    downvote_emoji VARCHAR(255) DEFAULT "❎" NOT NULL,
    ping TEXT,

    PRIMARY KEY (map_id)
);
