-- Frozen pre-scope grant shape from migration 24. Parameters bind the unchanged
-- target identity and snapshot; no ownership-scope acknowledgment existed.
INSERT INTO external_update_adoptions
    (target_path, consent_id, identity_fingerprint, review_fingerprint, reviewed_target_json)
VALUES ('/Applications/Example.app', '550e8400-e29b-41d4-a716-446655440088', ?1, 'legacy-review', ?2);
