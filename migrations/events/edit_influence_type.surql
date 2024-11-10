DEFINE EVENT OVERWRITE edit_influence_type ON TABLE influenced_by 
WHEN 
    $session.tk.ID == "backend"
    AND $event == "UPDATE"
    AND $before.influence_type != $after.influence_type 
THEN (
    CREATE activity 
    SET user = $after.in, 
        created_at = time::now(), 
        event_type = "EDIT_INFLUENCE_TYPE", 
        influence_type= $after.influence_type,
        influence = {
            id: $after.id,
            out: $after.out,
        }
);
