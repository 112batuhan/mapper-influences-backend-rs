DEFINE TABLE OVERWRITE activity_preference SCHEMAFULL;

DEFINE FIELD OVERWRITE login ON activity_preference TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE add_influence ON activity_preference TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE remove_influence ON activity_preference TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE add_user_beatmap ON activity_preference TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE remove_user_beatmap ON activity_preference TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE add_influence_beatmap ON activity_preference TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE remove_influence_beatmap ON activity_preference TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE edit_influence_description ON activity_preference TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE edit_influence_type ON activity_preference TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE edit_bio ON activity_preference TYPE bool DEFAULT true;
