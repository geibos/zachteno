-- Очередь задач, аудит, журнал попытки, служебный статус (spec audit-log, platform-operations).

CREATE TABLE jobs (
    id            bigserial PRIMARY KEY,
    kind          text NOT NULL,
    payload       jsonb NOT NULL DEFAULT '{}',
    run_at        timestamptz NOT NULL DEFAULT now(),
    attempts      integer NOT NULL DEFAULT 0,
    last_error    text,
    locked_until  timestamptz,
    done_at       timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_ready_idx ON jobs (run_at) WHERE done_at IS NULL;

CREATE TABLE audit_log (
    id           bigserial PRIMARY KEY,
    actor_id     uuid REFERENCES users (id) ON DELETE SET NULL,
    action       text NOT NULL,
    object_type  text NOT NULL,
    object_id    uuid,
    course_id    uuid,
    details      jsonb NOT NULL DEFAULT '{}',
    ip           inet,
    at           timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_at_idx ON audit_log (at DESC);
CREATE INDEX audit_log_course_idx ON audit_log (course_id, at DESC);

CREATE TABLE attempt_events (
    id           bigserial PRIMARY KEY,
    attempt_id   uuid NOT NULL REFERENCES attempts (id) ON DELETE CASCADE,
    kind         text NOT NULL,
    question_id  uuid,
    details      jsonb NOT NULL DEFAULT '{}',
    at           timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX attempt_events_attempt_idx ON attempt_events (attempt_id, at);

CREATE TABLE ops_status (
    name             text PRIMARY KEY,
    last_success_at  timestamptz,
    last_failure_at  timestamptz,
    details          text NOT NULL DEFAULT ''
);

-- Аудит неизменяем для роли приложения (spec audit-log, «Журнал действий»).
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'zachteno_app') THEN
        GRANT USAGE ON SCHEMA public TO zachteno_app;
        GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO zachteno_app;
        GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO zachteno_app;
        REVOKE UPDATE, DELETE, TRUNCATE ON audit_log FROM zachteno_app;
        REVOKE UPDATE, DELETE, TRUNCATE ON attempt_events FROM zachteno_app;
    END IF;
END $$;
