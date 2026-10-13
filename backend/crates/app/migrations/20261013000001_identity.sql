-- Учётные записи, роли, сессии, одноразовые токены (spec identity-auth, access-control).

CREATE TABLE users (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email                  text NOT NULL,
    email_lower            text GENERATED ALWAYS AS (lower(email)) STORED,
    display_name           text NOT NULL CHECK (length(display_name) BETWEEN 1 AND 200),
    password_hash          text,
    pepper_version         integer,
    email_verified_at      timestamptz,
    trusted_registration   boolean NOT NULL DEFAULT false,
    blocked_at             timestamptz,
    failed_logins          integer NOT NULL DEFAULT 0,
    last_failed_login_at   timestamptz,
    created_at             timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_email_lower_key UNIQUE (email_lower)
);

CREATE TABLE user_roles (
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role        text NOT NULL CHECK (role IN ('superuser', 'methodist')),
    granted_by  uuid REFERENCES users (id),
    granted_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, role)
);

CREATE TABLE sessions (
    token_hash    bytea PRIMARY KEY,
    id            uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    user_agent    text,
    ip            inet
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

CREATE TABLE external_identities (
    provider    text NOT NULL,
    subject     text NOT NULL,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email       text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (provider, subject)
);
CREATE INDEX external_identities_user_idx ON external_identities (user_id);

CREATE TABLE auth_tokens (
    token_hash  bytea PRIMARY KEY,
    kind        text NOT NULL CHECK (kind IN ('email_verify', 'password_reset', 'set_password')),
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at  timestamptz NOT NULL,
    used_at     timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE oauth_states (
    state_hash     bytea PRIMARY KEY,
    provider       text NOT NULL,
    pkce_verifier  text NOT NULL,
    link_user_id   uuid REFERENCES users (id) ON DELETE CASCADE,
    invite_hash    bytea,
    trusted_hash   bytea,
    created_at     timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE trusted_links (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash  bytea NOT NULL UNIQUE,
    label       text NOT NULL DEFAULT '',
    created_by  uuid NOT NULL REFERENCES users (id),
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE trusted_link_uses (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    link_id   uuid NOT NULL REFERENCES trusted_links (id) ON DELETE CASCADE,
    user_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    used_at   timestamptz NOT NULL DEFAULT now(),
    ip        inet
);
