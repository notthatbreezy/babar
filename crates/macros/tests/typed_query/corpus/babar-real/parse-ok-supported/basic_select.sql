SELECT users.id, users.name
FROM users /* active users only */
WHERE users.id = $id AND users.deleted_at IS NULL
ORDER BY users.name DESC
LIMIT 10
OFFSET 5;
