DEFINE EVENT OVERWRITE remove_user_beatmap ON TABLE user WHEN $event == "UPDATE" AND  array::len($before.beatmaps) > array::len($after.beatmaps) THEN (
    CREATE activity 
    SET user = $after.id, 
        created_at = time::now(),
        event_type = "REMOVE_USER_BEATMAP", 
        beatmap = array::complement($before.beatmaps, $after.beatmaps).at(0)
);
