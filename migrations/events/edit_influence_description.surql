DEFINE EVENT OVERWRITE edit_influence_description ON TABLE influenced_by
WHEN
    $session.tk.ID == "backend"
    AND $event == "UPDATE"
    AND $before.description != $after.description
THEN (
    CREATE activity 
    SET user = $after.in, 
        created_at = time::now(), 
        event_type = "EDIT_INFLUENCE_DESC", 
        description = $after.description,
        influence = {
            id: $after.id,
            out: $after.out,
        }
);
