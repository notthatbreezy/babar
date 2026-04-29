SELECT u.id, p.name AS pet_name
FROM users AS u
LEFT JOIN pets AS p ON u.id = p.user_id
WHERE p.name = $pet_name OR p.adopted = TRUE
ORDER BY p.name ASC NULLS LAST;
