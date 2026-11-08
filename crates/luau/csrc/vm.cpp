// The engine's side of Luau's VM (docs/STUDIO-2.md §4.13, from spike S0's
// shim): a capped allocator, the interrupt budget, the sandbox, loading
// bytecode, and a trampoline so Rust functions never unwind: a Rust function
// returns -1 to raise the message it left on the stack, and lua_error is
// called from here, with the position of the Lua line that called it.
#include "lua.h"
#include "lualib.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

namespace
{
struct SvState
{
    size_t used = 0;
    // The heap's limit, checked at interrupts (safe points in the VM loop),
    // and the hard cap the allocator enforces, a margin above it: so the
    // limit is met as a Lua error from Lua code, and an allocation made by
    // Rust between Lua calls (a binding's table) doesn't fail.
    size_t limit = 0;
    size_t hard = 0;
    size_t peak = 0;
    int64_t left = 0; // interrupts left this frame
    int64_t budget = 0;
    int64_t counted = 0; // interrupts seen since the last reset
    int tripped = 0;
    int metered = 0;
    void* host = nullptr;
    char msg[320] = {};
};

SvState* sv(lua_State* L)
{
    return static_cast<SvState*>(lua_callbacks(L)->userdata);
}

void* sv_alloc(void* ud, void* ptr, size_t osize, size_t nsize)
{
    SvState* s = static_cast<SvState*>(ud);
    if (nsize == 0)
    {
        free(ptr);
        s->used -= osize;
        return nullptr;
    }
    if (nsize > osize && s->used - osize + nsize > s->hard)
        return nullptr; // Luau raises "not enough memory"
    void* p = realloc(ptr, nsize);
    if (p)
    {
        s->used = s->used - osize + nsize;
        if (s->used > s->peak)
            s->peak = s->used;
    }
    return p;
}

// Called at loop back-edges, calls and returns (the same places every run),
// so counting them is a deterministic budget.
void sv_interrupt(lua_State* L, int gc)
{
    if (gc >= 0)
        return;
    SvState* s = sv(L);
    if (s->used > s->limit)
    {
        // Over the heap's limit: an error here, where Lua is running (each
        // later interrupt raises again until memory is freed).
        char m[160];
        luaL_where(L, 0);
        snprintf(m, sizeof(m), "%snot enough memory: the round's Lua heap is over %zu MB", lua_tostring(L, -1) ? lua_tostring(L, -1) : "", s->limit >> 20);
        lua_pop(L, 1);
        lua_pushstring(L, m);
        lua_error(L);
    }
    s->counted++;
    if (!s->metered || --s->left >= 0)
        return;
    if (!s->tripped)
    {
        s->tripped = 1;
        luaL_where(L, 0); // no C frame: level 0 is the interrupted Lua function
        const char* where = lua_tostring(L, -1);
        snprintf(s->msg, sizeof(s->msg), "%ssandbox: instruction budget exhausted (a loop that never ends?)", where ? where : "");
        lua_pop(L, 1);
    }
    // Every later interrupt raises again, so a pcall can catch it but the
    // code around the pcall stops at its next back-edge or call.
    lua_pushstring(L, s->msg);
    lua_error(L);
}

typedef int (*sv_rfn)(lua_State* L, void* ud);

// The allocator's hard cap above the heap's limit (SvState::hard).
const size_t kHeadroom = 8u << 20;

// sv_protect's call: the Rust function and its data, and what it returned.
struct SvProtected
{
    sv_rfn f;
    void* ud;
    int result;
};

int sv_protected(lua_State* L)
{
    SvProtected* p = static_cast<SvProtected*>(lua_tolightuserdata(L, 1));
    lua_pop(L, 1);
    p->result = p->f(L, p->ud);
    return 0;
}

int sv_trampoline(lua_State* L)
{
    sv_rfn f = reinterpret_cast<sv_rfn>(lua_tolightuserdata(L, lua_upvalueindex(1)));
    void* ud = lua_tolightuserdata(L, lua_upvalueindex(2));
    int n = f(L, ud);
    if (n == -2)
        lua_error(L); // an error from Lua the function called, already positioned
    if (n < 0)
    {
        // As luaL_error does: the position of the Lua code that called us.
        luaL_where(L, 1);
        lua_insert(L, -2);
        lua_concat(L, 2);
        lua_error(L);
    }
    return n;
}

// Globals a Studio game never gets (docs/STUDIO.md "Determinism").
const char* const kRemoved[] = {"getfenv", "setfenv", "gcinfo", "loadstring", "collectgarbage", "newproxy", "dofile", "loadfile", "load", "require", nullptr};
} // namespace

extern "C" {

lua_State* sv_new(size_t mem_limit)
{
    SvState* s = new SvState();
    s->limit = mem_limit;
    s->hard = mem_limit + kHeadroom;
    lua_State* L = lua_newstate(sv_alloc, s);
    if (!L)
    {
        delete s;
        return nullptr;
    }
    lua_callbacks(L)->userdata = s;
    static const luaL_Reg libs[] = {
        {"", luaopen_base},
        {LUA_COLIBNAME, luaopen_coroutine},
        {LUA_TABLIBNAME, luaopen_table},
        {LUA_STRLIBNAME, luaopen_string},
        {LUA_MATHLIBNAME, luaopen_math},
        {LUA_UTF8LIBNAME, luaopen_utf8},
        {LUA_BITLIBNAME, luaopen_bit32},
        {LUA_BUFFERLIBNAME, luaopen_buffer},
        {LUA_VECLIBNAME, luaopen_vector},
        {nullptr, nullptr},
    };
    for (const luaL_Reg* lib = libs; lib->func; lib++)
    {
        lua_pushcfunction(L, lib->func, nullptr);
        lua_pushstring(L, lib->name);
        lua_call(L, 1, 0);
    }
    for (const char* const* g = kRemoved; *g; g++)
    {
        lua_pushnil(L);
        lua_setglobal(L, *g);
    }
    lua_callbacks(L)->interrupt = sv_interrupt;
    return L;
}

void sv_close(lua_State* L)
{
    SvState* s = sv(L);
    lua_close(L);
    delete s;
}

void sv_set_host(lua_State* L, void* host)
{
    sv(L)->host = host;
}

void* sv_host(lua_State* L)
{
    return sv(L)->host;
}

// luaL_sandbox (linit.cpp, MIT, Roblox): every library table and the
// string metatable read-only, the globals read-only and "safe" (so builtins
// take their fast paths). Then a thread whose globals are a fresh table
// reading through to them, where the game's code runs.
lua_State* sv_sandbox(lua_State* L)
{
    lua_pushnil(L);
    while (lua_next(L, LUA_GLOBALSINDEX) != 0)
    {
        if (lua_istable(L, -1))
            lua_setreadonly(L, -1, true);
        lua_pop(L, 1);
    }
    lua_pushliteral(L, "");
    if (lua_getmetatable(L, -1))
    {
        lua_setreadonly(L, -1, true);
        lua_pop(L, 2);
    }
    else
        lua_pop(L, 1);
    lua_pushvector(L, 0.0f, 0.0f, 0.0f);
    if (lua_getmetatable(L, -1))
    {
        lua_setreadonly(L, -1, true);
        lua_pop(L, 2);
    }
    else
        lua_pop(L, 1);
    lua_setreadonly(L, LUA_GLOBALSINDEX, true);
    lua_setsafeenv(L, LUA_GLOBALSINDEX, true);

    lua_State* T = lua_newthread(L); // stays on L's stack, which keeps it alive
    // luaL_sandboxthread: a global table proxying reads to the read-only one.
    lua_newtable(T);
    lua_newtable(T);
    lua_pushvalue(T, LUA_GLOBALSINDEX);
    lua_setfield(T, -2, "__index");
    lua_setreadonly(T, -1, true);
    lua_setmetatable(T, -2);
    lua_replace(T, LUA_GLOBALSINDEX);
    lua_setsafeenv(T, LUA_GLOBALSINDEX, true);
    return T;
}

// Loads bytecode compiled ahead of time: 0 and the function on the stack,
// or 1 and the error message.
int sv_load_bytecode(lua_State* L, const char* chunkname, const char* bc, size_t len)
{
    return luau_load(L, chunkname, bc, len, 0) == 0 ? 0 : 1;
}

void sv_push_rfn(lua_State* L, sv_rfn f, void* ud, const char* name)
{
    lua_pushlightuserdata(L, reinterpret_cast<void*>(f));
    lua_pushlightuserdata(L, ud);
    lua_pushcclosure(L, sv_trampoline, name, 2);
}

void sv_set_budget(lua_State* L, int64_t n)
{
    SvState* s = sv(L);
    s->budget = n;
    s->left = n;
    s->metered = n > 0;
    s->tripped = 0;
    s->msg[0] = 0;
}

// The start of a frame: a full budget again (a trip lasts one frame, as
// 1.0's does: the frame's error ends the round).
void sv_reset_budget(lua_State* L)
{
    SvState* s = sv(L);
    s->left = s->budget;
    s->tripped = 0;
    s->msg[0] = 0;
    s->counted = 0;
}

int64_t sv_counted(lua_State* L)
{
    return sv(L)->counted;
}

const char* sv_trip_message(lua_State* L)
{
    SvState* s = sv(L);
    return s->tripped ? s->msg : nullptr;
}

// Runs a Rust function in protected mode on L's thread (lua_cpcall): a
// raise from the Lua API it calls (out of memory past the hard cap, a
// metamethod's error) ends the call with that status, instead of
// unwinding to no handler. Returns the status; `*result` is what the
// function returned when it finished.
int sv_protect(lua_State* L, sv_rfn f, void* ud, int* result)
{
    SvProtected p = {f, ud, 0};
    int status = lua_cpcall(L, sv_protected, &p);
    *result = p.result;
    return status;
}

// The heap's limit (tests make it small to meet it).
void sv_set_mem_limit(lua_State* L, size_t limit)
{
    SvState* s = sv(L);
    s->limit = limit;
    s->hard = limit + kHeadroom;
}

size_t sv_mem_used(lua_State* L)
{
    return sv(L)->used;
}

size_t sv_mem_peak(lua_State* L)
{
    return sv(L)->peak;
}
}
