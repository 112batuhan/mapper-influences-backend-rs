DEFINE TABLE OVERWRITE influenced_by SCHEMAFULL TYPE RELATION IN user OUT user ENFORCED;

DEFINE FIELD OVERWRITE influence_type on influenced_by TYPE int DEFAULT 1;
DEFINE FIELD OVERWRITE description ON influenced_by TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE beatmaps ON influenced_by TYPE set<int> DEFAULT [];
DEFINE FIELD OVERWRITE updated_at ON influenced_by type datetime VALUE time::now();
DEFINE FIELD OVERWRITE created_at ON influenced_by type datetime VALUE time::now() READONLY;

DEFINE FIELD OVERWRITE order on influenced_by TYPE int 
DEFAULT IF array::is_empty($this.in->influenced_by.order){
        0
    }ELSE{
        array::max($this.in->influenced_by.order) + 1
    };

// TO REMEMBER THIS BEAUTIFUL THING (AND AS A BACKUP) 
// COUNTLESS HOURS LOST BECAUSE I USED VALUE INSTEAD OF DEFAULT
// you can use this too, arguably a better solution performance wise. BUT IDC I WANT MY ORIGINAL SOLUTION
// DEFAULT (UPSERT type::thing("increment", $id) SET increment+=1 RETURN increment).at(0).values().at(0);

DEFINE INDEX unique_in_out ON TABLE influenced_by COLUMNS in, out UNIQUE;
