-- Режимы частичного начисления для множественного выбора (spec scoring,
-- change add-partial-credit-scoring).

ALTER TABLE questions DROP CONSTRAINT questions_kind_check;

ALTER TABLE questions ADD CONSTRAINT questions_kind_check CHECK (kind IN (
    'single', 'multiple_all', 'multiple_any', 'multiple_each', 'multiple_fraction', 'free_text'
));
