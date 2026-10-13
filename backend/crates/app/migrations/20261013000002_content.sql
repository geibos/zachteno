-- Курсы, тесты, вопросы, варианты, вложения (spec course-authoring, scoring).

CREATE TABLE attachments (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    storage_key   text NOT NULL UNIQUE,
    content_type  text NOT NULL,
    byte_size     bigint NOT NULL,
    width         integer NOT NULL,
    height        integer NOT NULL,
    uploaded_by   uuid REFERENCES users (id),
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE courses (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    title        text NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
    description  text NOT NULL DEFAULT '',
    owner_id     uuid NOT NULL REFERENCES users (id),
    archived_at  timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE course_teachers (
    course_id    uuid NOT NULL REFERENCES courses (id) ON DELETE CASCADE,
    user_id      uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    assigned_by  uuid REFERENCES users (id),
    assigned_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (course_id, user_id)
);

CREATE TABLE tests (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    course_id              uuid NOT NULL REFERENCES courses (id) ON DELETE CASCADE,
    title                  text NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
    description            text NOT NULL DEFAULT '',
    status                 text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    position               integer NOT NULL DEFAULT 0,
    attempts_limit         integer CHECK (attempts_limit >= 1),
    time_limit_minutes     integer CHECK (time_limit_minutes >= 1),
    questions_per_attempt  integer CHECK (questions_per_attempt >= 1),
    free_navigation        boolean NOT NULL DEFAULT false,
    shuffle_questions      boolean NOT NULL DEFAULT false,
    shuffle_options        boolean NOT NULL DEFAULT false,
    review_policy          text NOT NULL DEFAULT 'after_submit' CHECK (review_policy IN ('never', 'after_submit', 'after_close')),
    review_not_before      timestamptz,
    final_policy           text NOT NULL DEFAULT 'best' CHECK (final_policy IN ('best', 'last', 'average')),
    locked_at              timestamptz,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX tests_course_idx ON tests (course_id, position);

CREATE TABLE questions (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    test_id     uuid NOT NULL REFERENCES tests (id) ON DELETE CASCADE,
    position    integer NOT NULL,
    kind        text NOT NULL CHECK (kind IN ('single', 'multiple_all', 'multiple_any', 'free_text')),
    body        text NOT NULL DEFAULT '',
    weight      numeric(8, 2) NOT NULL DEFAULT 1 CHECK (weight > 0),
    image_id    uuid REFERENCES attachments (id),
    image_alt   text NOT NULL DEFAULT '',
    hint        text NOT NULL DEFAULT ''
);
CREATE INDEX questions_test_idx ON questions (test_id, position);

CREATE TABLE options (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    question_id  uuid NOT NULL REFERENCES questions (id) ON DELETE CASCADE,
    position     integer NOT NULL,
    body         text NOT NULL DEFAULT '',
    image_id     uuid REFERENCES attachments (id),
    image_alt    text NOT NULL DEFAULT '',
    is_correct   boolean NOT NULL DEFAULT false,
    penalty      numeric(8, 2) NOT NULL DEFAULT 0 CHECK (penalty <= 0),
    penalty_custom boolean NOT NULL DEFAULT false,
    pinned       boolean NOT NULL DEFAULT false
);
CREATE INDEX options_question_idx ON options (question_id, position);
