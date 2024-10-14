DEFINE TABLE OVERWRITE user SCHEMAFULL;

DEFINE FIELD OVERWRITE username ON user TYPE string;
DEFINE FIELD OVERWRITE avatar_url ON user TYPE string;
DEFINE FIELD OVERWRITE country ON user TYPE string;
DEFINE FIELD OVERWRITE bio ON user TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE ranked_mapper ON user TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE authenticated ON user TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE beatmaps ON user TYPE set<int> DEFAULT [];
DEFINE FIELD OVERWRITE updated_at ON user type datetime VALUE time::now();
DEFINE FIELD OVERWRITE created_at ON user type datetime VALUE time::now() READONLY;
