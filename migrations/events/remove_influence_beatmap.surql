DEFINE EVENT OVERWRITE remove_influence_beatmap ON TABLE influenced_by
WHEN
    $session.tk.ID == "backend"
    AND $event == "UPDATE"
    AND array::len($before.beatmaps) > array::len($after.beatmaps)
THEN (
    CREATE activity 
    SET user = $after.in, 
        created_at = time::now(),
        event_type = "REMOVE_INFLUENCE_BEATMAP", 
        beatmap = array::complement($before.beatmaps, $after.beatmaps).at(0),
        influence = {
            id: $after.id,
            out: $after.out,
        }
);
