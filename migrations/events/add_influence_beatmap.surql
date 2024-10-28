DEFINE EVENT OVERWRITE add_influence_beatmap ON TABLE influenced_by WHEN $event == "UPDATE" AND array::len($after.beatmaps) > array::len($before.beatmaps) THEN (
    CREATE activity 
    SET user = $after.in,
        created_at = time::now(),
        event_type = "ADD_INFLUENCE_BEATMAP", 
        beatmap = array::complement($after.beatmaps, $before.beatmaps).at(0),
        influence = {
            id: $after.id,
            out: $after.out,
        }
);
