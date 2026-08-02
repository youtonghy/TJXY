-- Seeds a rich, realistic watch history for the admin user.
--
-- Drives two independent surfaces:
--   * playback_sessions -> profile insights (watch time, daily chart, genres, recent)
--   * user_data         -> played marks, favorites, resume ("continue watching")
--
-- Idempotent: clears the admin user's prior seeded rows first.

BEGIN;

-- Resolve identities once.
CREATE TEMP TABLE seed_ctx AS
SELECT
  (SELECT id FROM users WHERE username = 'admin') AS user_id,
  (SELECT id FROM auth_sessions
    WHERE user_id = (SELECT id FROM users WHERE username = 'admin')
    ORDER BY id LIMIT 1) AS auth_session_id;

-- Candidate pool: movies that actually have a playable source, genre-tagged first
-- so the genre breakdown on the profile is populated.
CREATE TEMP TABLE seed_pool AS
SELECT ci.id,
       ci.runtime_ticks,
       (ci.id IN (SELECT catalog_item_id FROM item_genres)) AS has_genre,
       row_number() OVER (
         ORDER BY (ci.id IN (SELECT catalog_item_id FROM item_genres)) DESC, ci.id
       ) AS rn
FROM catalog_items ci
JOIN (SELECT DISTINCT catalog_item_id FROM media_sources) ms
  ON ms.catalog_item_id = ci.id
WHERE ci.item_type = 'Movie'
  AND ci.is_present = true;

-- Reset previous seed for this user.
DELETE FROM playback_sessions
WHERE user_id = (SELECT user_id FROM seed_ctx);
DELETE FROM user_data
WHERE user_id = (SELECT user_id FROM seed_ctx);

-- ---------------------------------------------------------------------------
-- playback_sessions: 420 sessions spread over the last 180 days.
-- Recent days are weighted more heavily so "this week / this month" ranges
-- all return non-empty data.
-- ---------------------------------------------------------------------------
INSERT INTO playback_sessions (
  id, auth_session_id, play_session_id, user_id, catalog_item_id,
  presentation_key, last_position_ticks, started_at, last_event_at,
  stopped_at, watched_ticks
)
SELECT
  gen_random_uuid(),
  ctx.auth_session_id,
  gen_random_uuid(),
  ctx.user_id,
  p.id,
  gen_random_uuid(),
  watched.ticks,
  started.at,
  started.at + (watched.ticks / 10000000.0) * INTERVAL '1 second',
  started.at + (watched.ticks / 10000000.0) * INTERVAL '1 second',
  watched.ticks
FROM seed_ctx ctx
CROSS JOIN generate_series(1, 420) AS s(n)
JOIN seed_pool p
  -- cycle through the genre-tagged head of the pool, with drift so the
  -- same title repeats occasionally (gives play_count > 1 realism)
  ON p.rn = 1 + ((s.n * 7) % GREATEST((SELECT COUNT(*) FROM seed_pool), 1))
CROSS JOIN LATERAL (
  SELECT now()
       - (CASE
            WHEN s.n <= 60  THEN (s.n % 7)          -- last week: dense
            WHEN s.n <= 180 THEN 7 + (s.n % 23)     -- last month
            ELSE 30 + (s.n % 150)                   -- older tail
          END) * INTERVAL '1 day'
       - ((s.n * 37) % 1440) * INTERVAL '1 minute'  AS at
) started
CROSS JOIN LATERAL (
  SELECT GREATEST(
    -- 60%-100% of runtime, defaulting to ~100 min when runtime is unknown
    ((COALESCE(NULLIF(p.runtime_ticks, 0), 60000000000) * (60 + (s.n % 41))) / 100)::bigint,
    3000000000::bigint
  ) AS ticks
) watched;

-- ---------------------------------------------------------------------------
-- user_data: finished titles, favorites, and in-progress resume points.
-- ---------------------------------------------------------------------------

-- 1) Fully watched (240 titles), some rewatched.
INSERT INTO user_data (
  id, user_id, catalog_item_id, playback_position_ticks,
  is_played, play_count, is_favorite, last_played_at, updated_at
)
SELECT
  gen_random_uuid(),
  ctx.user_id,
  p.id,
  0,
  true,
  1 + (p.rn % 3),                      -- 1..3 plays
  (p.rn % 5 = 0),                      -- every 5th is a favorite
  now() - (p.rn % 120) * INTERVAL '1 day',
  now() - (p.rn % 120) * INTERVAL '1 day'
FROM seed_ctx ctx
JOIN seed_pool p ON p.rn BETWEEN 1 AND 240
ON CONFLICT (user_id, catalog_item_id) DO NOTHING;

-- 2) In progress (30 titles) -> populates "continue watching".
INSERT INTO user_data (
  id, user_id, catalog_item_id, playback_position_ticks,
  is_played, play_count, is_favorite, last_played_at, updated_at
)
SELECT
  gen_random_uuid(),
  ctx.user_id,
  p.id,
  -- 15%-75% through the title
  ((COALESCE(NULLIF(p.runtime_ticks, 0), 60000000000) * (15 + (p.rn % 61))) / 100)::bigint,
  false,
  1,
  (p.rn % 7 = 0),
  now() - (p.rn % 14) * INTERVAL '1 day',
  now() - (p.rn % 14) * INTERVAL '1 day'
FROM seed_ctx ctx
JOIN seed_pool p ON p.rn BETWEEN 241 AND 270
ON CONFLICT (user_id, catalog_item_id) DO NOTHING;

-- 3) Favorites not yet watched (20 titles) -> watchlist behaviour.
INSERT INTO user_data (
  id, user_id, catalog_item_id, playback_position_ticks,
  is_played, play_count, is_favorite, last_played_at, updated_at
)
SELECT
  gen_random_uuid(),
  ctx.user_id,
  p.id,
  0,
  false,
  0,
  true,
  NULL,
  now() - (p.rn % 30) * INTERVAL '1 day'
FROM seed_ctx ctx
JOIN seed_pool p ON p.rn BETWEEN 271 AND 290
ON CONFLICT (user_id, catalog_item_id) DO NOTHING;

COMMIT;
