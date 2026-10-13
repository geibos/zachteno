-- Сигналы и связи антифрода (spec antifraud-signals, design D14).

CREATE TABLE signals (
    id                 bigserial PRIMARY KEY,
    user_id            uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    event              text NOT NULL,
    at                 timestamptz NOT NULL DEFAULT now(),
    ip                 inet,
    asn                integer,
    country            text,
    hosting            boolean NOT NULL DEFAULT false,
    tz_mismatch        boolean NOT NULL DEFAULT false,
    marker_hash        bytea,
    browser_hash       bytea,
    device_hash        bytea,
    device_components  jsonb NOT NULL DEFAULT '{}',
    typing             jsonb
);
CREATE INDEX signals_user_idx ON signals (user_id, at);
CREATE INDEX signals_marker_idx ON signals (marker_hash);
CREATE INDEX signals_device_idx ON signals (device_hash);
CREATE INDEX signals_ip_idx ON signals (ip);

CREATE TABLE identity_links (
    user_a       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    user_b       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    level        text NOT NULL CHECK (level IN ('low', 'medium', 'high')),
    evidence     jsonb NOT NULL DEFAULT '[]',
    computed_at  timestamptz NOT NULL DEFAULT now(),
    decision     text CHECK (decision IN ('confirmed', 'rejected')),
    decided_by   uuid REFERENCES users (id),
    decided_at   timestamptz,
    comment      text NOT NULL DEFAULT '',
    PRIMARY KEY (user_a, user_b),
    CHECK (user_a < user_b)
);
CREATE INDEX identity_links_b_idx ON identity_links (user_b);

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'zachteno_app') THEN
        GRANT SELECT, INSERT, UPDATE, DELETE ON signals, identity_links TO zachteno_app;
        GRANT USAGE, SELECT ON SEQUENCE signals_id_seq TO zachteno_app;
    END IF;
END $$;
