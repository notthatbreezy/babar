SELECT users.id
FROM users
WHERE users.id = $id OR users.manager_id = $id;
