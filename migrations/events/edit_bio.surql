// edit_bio logs when creating user, so added before != null
DEFINE EVENT OVERWRITE edit_bio ON TABLE user WHEN $event == "UPDATE" AND $before.bio != $after.bio THEN (
    CREATE activity 
    SET user = $after.id, 
        created_at = time::now(), 
        event_type = "EDIT_BIO", 
        bio = $after.bio
);
