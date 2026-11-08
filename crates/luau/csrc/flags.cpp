// Luau's feature flags, as its CLI sets them: every rolled-out "Luau*" flag
// on, Debug flags off. The VM and the compiler both read them, so the engine
// and `mb` turn them on the same way before anything else (luau::init).
#include "Luau/Common.h"

#include <string.h>

extern "C" {

void sv_enable_flags()
{
    for (Luau::FValue<bool>* flag = Luau::FValue<bool>::list; flag; flag = flag->next)
        if (strncmp(flag->name, "Luau", 4) == 0)
            flag->value = true;
}

int sv_flag_count()
{
    int n = 0;
    for (Luau::FValue<bool>* flag = Luau::FValue<bool>::list; flag; flag = flag->next)
        n++;
    return n;
}
}
