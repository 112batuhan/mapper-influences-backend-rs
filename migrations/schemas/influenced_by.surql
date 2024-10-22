DEFINE TABLE OVERWRITE influenced_by SCHEMAFULL TYPE RELATION IN user OUT user ENFORCED;

DEFINE FIELD OVERWRITE influence_type on influenced_by TYPE int DEFAULT 1;
DEFINE FIELD OVERWRITE description ON influenced_by TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE beatmaps ON influenced_by TYPE set<int> DEFAULT [];
DEFINE FIELD OVERWRITE updated_at ON influenced_by type datetime VALUE time::now();
DEFINE FIELD OVERWRITE created_at ON influenced_by type datetime VALUE time::now() READONLY;

// COUNTLESS HOURS LOST BECAUSE I USED VALUE INSTEAD OF DEFAULT
DEFINE FIELD OVERWRITE order on influenced_by TYPE int 
DEFAULT (UPSERT increment:order SET increment+=1 RETURN increment).at(0).values().at(0);

DEFINE INDEX OVERWRITE unique_in_out ON TABLE influenced_by COLUMNS in, out UNIQUE;
