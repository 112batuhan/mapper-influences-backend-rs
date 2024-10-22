DEFINE EVENT OVERWRITE add_user_beatmap ON TABLE user WHEN $event == "UPDATE" AND array::len($after.beatmaps) > array::len($before.beatmaps) THEN (
    CREATE activity 
    SET user = $after.id,
        created_at = time::now(),
        event_type = "ADD_USER_BEATMAP", 
        beatmap = array::complement($after.beatmaps, $before.beatmaps).at(0)
);
