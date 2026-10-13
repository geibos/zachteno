-- Потоки, приглашения, окна прохождения, попытки и ответы (spec streams-enrollment, test-taking).

CREATE TABLE streams (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    created_by  uuid NOT NULL REFERENCES users (id),
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE stream_courses (
    stream_id    uuid NOT NULL REFERENCES streams (id) ON DELETE CASCADE,
    course_id    uuid NOT NULL REFERENCES courses (id) ON DELETE CASCADE,
    assigned_by  uuid REFERENCES users (id),
    assigned_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (stream_id, course_id)
);
CREATE INDEX stream_courses_course_idx ON stream_courses (course_id);

CREATE TABLE stream_members (
    stream_id   uuid NOT NULL REFERENCES streams (id) ON DELETE CASCADE,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    joined_at   timestamptz NOT NULL DEFAULT now(),
    removed_at  timestamptz,
    removed_by  uuid REFERENCES users (id),
    PRIMARY KEY (stream_id, user_id)
);
CREATE INDEX stream_members_user_idx ON stream_members (user_id);

CREATE TABLE invite_links (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    stream_id   uuid NOT NULL REFERENCES streams (id) ON DELETE CASCADE,
    token_hash  bytea NOT NULL UNIQUE,
    label       text NOT NULL DEFAULT '',
    created_by  uuid NOT NULL REFERENCES users (id),
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE invite_link_uses (
    id       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    link_id  uuid NOT NULL REFERENCES invite_links (id) ON DELETE CASCADE,
    user_id  uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    used_at  timestamptz NOT NULL DEFAULT now(),
    ip       inet
);
CREATE INDEX invite_link_uses_link_idx ON invite_link_uses (link_id, used_at);

CREATE TABLE test_windows (
    test_id     uuid NOT NULL REFERENCES tests (id) ON DELETE CASCADE,
    stream_id   uuid NOT NULL REFERENCES streams (id) ON DELETE CASCADE,
    is_open     boolean NOT NULL DEFAULT false,
    open_at     timestamptz,
    close_at    timestamptz,
    closed_at   timestamptz,
    updated_by  uuid REFERENCES users (id),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (test_id, stream_id)
);

CREATE TABLE attempts (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    test_id        uuid NOT NULL REFERENCES tests (id) ON DELETE CASCADE,
    user_id        uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    stream_id      uuid REFERENCES streams (id),
    is_preview     boolean NOT NULL DEFAULT false,
    started_at     timestamptz NOT NULL DEFAULT now(),
    deadline_at    timestamptz,
    submitted_at   timestamptz,
    submit_reason  text CHECK (submit_reason IN ('manual', 'timeout', 'window_closed')),
    layout         jsonb NOT NULL,
    current_index  integer NOT NULL DEFAULT 0,
    lock_applied   boolean NOT NULL DEFAULT false,
    score          numeric(10, 2),
    max_score      numeric(10, 2) NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX attempts_one_active_idx ON attempts (test_id, user_id)
    WHERE submitted_at IS NULL AND NOT is_preview;
CREATE INDEX attempts_deadline_idx ON attempts (deadline_at) WHERE submitted_at IS NULL;
CREATE INDEX attempts_test_stream_idx ON attempts (test_id, stream_id);

CREATE TABLE answers (
    attempt_id           uuid NOT NULL REFERENCES attempts (id) ON DELETE CASCADE,
    question_id          uuid NOT NULL REFERENCES questions (id) ON DELETE CASCADE,
    selected_option_ids  uuid[] NOT NULL DEFAULT '{}',
    text_answer          text,
    client_seq           bigint NOT NULL DEFAULT 0,
    saved_at             timestamptz NOT NULL DEFAULT now(),
    finalized            boolean NOT NULL DEFAULT false,
    score                numeric(8, 2),
    PRIMARY KEY (attempt_id, question_id)
);
