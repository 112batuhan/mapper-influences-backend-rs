DEFINE FUNCTION OVERWRITE fn::id_or_null($value: any) -> any {
    IF $value.is_none() {
        RETURN NONE;
    }
    ELSE {
        RETURN meta::id($value);
    };
} PERMISSIONS FULL;

DEFINE FUNCTION OVERWRITE fn::add_possible_nulls($var1: any, $var2: any) -> any {
    IF type::is::none($var1) AND type::is::none($var2) {
        RETURN NONE;
    }
    ELSE {
        RETURN $var1 + $var2;
    };
} PERMISSIONS FULL;
