// wasm only: what the platform doesn't give a game (SPEC §3: no clock, no
// files). Defined here so the linker never pulls wasi-libc's versions, which
// import WASI functions.
#include "lua.h"

#include <time.h>

// lperf.cpp's clock: only the GC's metrics read it.
extern "C" double lua_clock()
{
    return 0.0;
}

// lmathlib.cpp seeds math.random from time(NULL); the engine replaces
// math.random with the round's PCG32 anyway.
extern "C" time_t time(time_t* t)
{
    if (t)
        *t = 0;
    return 0;
}

// The WASI functions wasi-libc imports, defined here so the game imports
// none: libunwind's debug logging (getenv), libc++abi's abort message and
// Luau's print and memory dump (stdio), mutex waits (a clock), abort (exit).
// Nothing a game runs reaches them except a print or an abort; output goes
// nowhere (the engine's print is the platform's mb_log) and exit traps.
#include <stdint.h>
extern "C" {
int32_t __imported_wasi_snapshot_preview1_fd_write(int32_t fd, int32_t iovs, int32_t n, int32_t nwritten)
{
    (void)fd;
    const int32_t* iov = reinterpret_cast<const int32_t*>(iovs);
    int32_t total = 0;
    for (int32_t i = 0; i < n; i++)
        total += iov[2 * i + 1];
    *reinterpret_cast<int32_t*>(nwritten) = total;
    return 0;
}
void __imported_wasi_snapshot_preview1_proc_exit(int32_t)
{
    __builtin_trap();
}
int32_t __imported_wasi_snapshot_preview1_clock_time_get(int32_t, int64_t, int32_t out)
{
    *reinterpret_cast<int64_t*>(out) = 0;
    return 0;
}
int32_t __imported_wasi_snapshot_preview1_environ_get(int32_t, int32_t)
{
    return 0;
}
int32_t __imported_wasi_snapshot_preview1_environ_sizes_get(int32_t count, int32_t size)
{
    *reinterpret_cast<int32_t*>(count) = 0;
    *reinterpret_cast<int32_t*>(size) = 0;
    return 0;
}
int32_t __imported_wasi_snapshot_preview1_fd_close(int32_t)
{
    return 8; // EBADF
}
int32_t __imported_wasi_snapshot_preview1_fd_fdstat_get(int32_t, int32_t)
{
    return 8;
}
int32_t __imported_wasi_snapshot_preview1_fd_seek(int32_t, int64_t, int32_t, int32_t)
{
    return 70; // ESPIPE
}
}

// The same seed mixes in clock().
extern "C" clock_t clock()
{
    return 0;
}
