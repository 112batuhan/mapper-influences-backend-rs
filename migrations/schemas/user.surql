DEFINE TABLE OVERWRITE user SCHEMAFULL;

DEFINE FIELD OVERWRITE username ON user TYPE string;
DEFINE FIELD OVERWRITE avatar_url ON user TYPE string;
DEFINE FIELD OVERWRITE bio ON user TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE ranked_mapper ON user TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE authenticated ON user TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE beatmaps ON user TYPE set<int> DEFAULT [];
DEFINE FIELD OVERWRITE updated_at ON user type datetime VALUE time::now();
DEFINE FIELD OVERWRITE created_at ON user type datetime VALUE time::now() READONLY;
DEFINE FIELD OVERWRITE country_name ON user TYPE string;
DEFINE FIELD OVERWRITE country_code ON user TYPE string;
DEFINE FIELD OVERWRITE groups ON user FLEXIBLE TYPE array<object>;
DEFINE FIELD OVERWRITE previous_usernames ON user TYPE array<string>;
DEFINE FIELD OVERWRITE ranked_and_approved_beatmapset_count ON user TYPE int;
DEFINE FIELD OVERWRITE ranked_beatmapset_count ON user TYPE int;
DEFINE FIELD OVERWRITE nominated_beatmapset_count ON user TYPE int;
DEFINE FIELD OVERWRITE guest_beatmapset_count ON user TYPE int;
DEFINE FIELD OVERWRITE loved_beatmapset_count ON user TYPE int;
DEFINE FIELD OVERWRITE graveyard_beatmapset_count ON user TYPE int;
DEFINE FIELD OVERWRITE pending_beatmapset_count ON user TYPE int;

DEFINE FIELD OVERWRITE activity_preferences ON user FLEXIBLE TYPE object 
DEFAULT{
    add_influence: true,
    add_influence_beatmap: true,
    add_user_beatmap: true,
    edit_bio: true,
    edit_influence_description: true,
    edit_influence_type: true,
    login: false,
    remove_influence: false,
    remove_influence_beatmap: false,
    remove_user_beatmap: false,
};
