DEFINE EVENT OVERWRITE add_influence ON TABLE influenced_by WHEN $event == "CREATE" THEN (
    CREATE activity 
    SET user = $after.in, 
        created_at = time::now(),
        event_type = "ADD_INFLUENCE", 
        influence = {
    		id: $after.id,
    		out: $after.out,       
            }
);
