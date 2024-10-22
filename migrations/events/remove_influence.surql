DEFINE EVENT OVERWRITE remove_influence ON TABLE influenced_by WHEN $event == "DELETE" THEN (
    CREATE activity 
    SET user = $before.in, 
        created_at = time::now(),
        event_type = "REMOVE_INFLUENCE", 
        influence = $before
);
