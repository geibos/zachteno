-- Запрет копирования текста студентом (spec course-authoring, test-taking,
-- change protect-test-content).

ALTER TABLE tests ADD COLUMN copy_policy text NOT NULL DEFAULT 'allowed'
    CHECK (copy_policy IN ('allowed', 'forbidden'));
