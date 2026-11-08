// Compiling Lua to Luau bytecode with the engine's options (docs/STUDIO-2.md
// §4.13, Decided 36): `mb build` and the server compile a project's chunks
// with this, and the engine's build its own Lua. The engine has no compiler.
#include "luacode.h"

#include <stdlib.h>

extern "C" {

// Every builtin the engine wraps. In a sandboxed ("safe") environment a call
// the compiler knows as a builtin runs the builtin itself (FASTCALL),
// whatever the global holds, so a wrapper would be silently skipped.
static const char* const kDisabled[] = {"setmetatable", nullptr};

// The bytecode for a chunk; free() it after. A syntax error comes back as
// bytecode whose first byte is 0, followed by ":<line>: <message>".
char* sv_compile(const char* src, size_t len, size_t* outlen)
{
    lua_CompileOptions o = {};
    o.optimizationLevel = 2;
    o.debugLevel = 1; // line info, so runtime errors name the source line
    o.disabledBuiltins = kDisabled;
    return luau_compile(src, len, &o, outlen);
}
}
