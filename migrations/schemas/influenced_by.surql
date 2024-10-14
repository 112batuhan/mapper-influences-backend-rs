DEFINE TABLE OVERWRITE influenced_by SCHEMAFULL TYPE RELATION IN user OUT user ENFORCED;

DEFINE FIELD OVERWRITE order on influenced_by TYPE int; 
DEFINE FIELD OVERWRITE influence_type on influenced_by TYPE int DEFAULT 1;
DEFINE FIELD OVERWRITE description ON influenced_by TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE beatmaps ON influenced_by TYPE set<int> DEFAULT [];
DEFINE FIELD OVERWRITE updated_at ON influenced_by type datetime VALUE time::now();
DEFINE FIELD OVERWRITE created_at ON influenced_by type datetime VALUE time::now() READONLY;
